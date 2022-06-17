extern crate clap;
extern crate serde_json;
extern crate toml;
extern crate url;
#[macro_use]
extern crate serde;
extern crate rocksdb;
#[macro_use]
extern crate log;

mod paypal;

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};
use async_channel::Sender;
use rocksdb::{DBWithThreadMode, IteratorMode, MultiThreaded};
use serde_json::json;
use smartlike_embed_lib::client::{Client, DonationReceipt};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{fs::File, io::prelude::*, time::Duration};

const WAIT_SECONDS_BEFORE_RESEND: u64 = 5;

#[derive(Deserialize, Serialize, Default, Debug)]
pub struct Configuration {
    listen_address: String,
    num_threads: usize,
    network_address: String,
    smartlike_account: String,
    smartlike_key: String,
}

async fn paypal_handler(
    text: String,
    tx: web::Data<Sender<(String, String)>>,
    db: web::Data<DBWithThreadMode<MultiThreaded>>,
) -> actix_web::Result<HttpResponse> {
    match web::Query::from_query(&text) {
        Ok(q) => {
            match paypal::parse(&text, q).await {
                Ok(receipt) => {
                    // Store the receipt until it's successfully processed.
                    let msg = serde_json::to_string(&receipt).unwrap();
                    match db.put(receipt.id.as_str(), msg.as_str()) {
                        Ok(_) => {
                            // Perform async forwarding to return IPN faster.
                            tx.send((receipt.id, msg))
                                .await
                                .unwrap_or_else(|e| panic!("TX error: {}", e));
                            Ok(HttpResponse::Ok()
                                .content_type("text/plain")
                                .body("".to_string()))
                        }
                        Err(e) => panic!("DB error: {}", e),
                    }
                }
                Err(e) => {
                    error!("Failed to parse IPN: {} {}", text, e);
                    Ok(HttpResponse::Ok().content_type("text/plain").body("Error"))
                }
            }
        }
        Err(e) => {
            error!("Failed to parse query string: {}", e);
            Ok(HttpResponse::Ok().content_type("text/plain").body("Error"))
        }
    }
}

async fn test_ping_handler(
    query: web::Query<HashMap<String, String>>,
    client: web::Data<Client>,
) -> actix_web::Result<HttpResponse> {
    if query.contains_key("token") {
        let token = query.get("token").unwrap();
        let signature = client.sign(&token);
        debug!("{:?}", token);
        Ok(HttpResponse::Ok()
            .content_type("text/plain")
            .body(json!({ "token": token, "signature": signature }).to_string()))
    } else {
        Ok(HttpResponse::Ok().content_type("text/plain").body(""))
    }
}

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    let matches = clap::App::new("smartlike-gateway")
        .version(option_env!("CARGO_PKG_VERSION").unwrap_or(""))
        .about("smartlike-gateway")
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Configuration file")
                .takes_value(true)
                .default_value(""),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap();
    let mut f = File::open(config).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();

    let config = toml::from_str::<Configuration>(&contents)
        .map_err(|e| format!("Error loading configuration: {}", e.to_string()))
        .unwrap();

    let db = web::Data::new(
        DBWithThreadMode::<MultiThreaded>::open_default("./pending_receipts").unwrap(),
    );

    let client = Client::new(
        config.smartlike_account,
        config.smartlike_key,
        config.network_address,
    );

    let (tx, rx) = async_channel::unbounded::<(String, DonationReceipt)>();

    // Load pending receipts from previous runs and retry them.
    let iter = db.iterator(IteratorMode::Start);
    for (key, value) in iter {
        trace!("Found pending request {:?} {:?}", key, value);
        if let (Ok(k), Ok(v)) = (
            String::from_utf8(key.to_vec()),
            String::from_utf8(value.to_vec()),
        ) {
            if let Ok(receipt) = serde_json::from_str(&v) {
                tx.send((k, receipt))
                    .await
                    .unwrap_or_else(|e| panic!("TX error: {}", e));
                continue;
            }
        }
        error!(
            "Failed to enqueue pending receipt. Rejecting {:?} {:?}.",
            key, value
        );
        db.delete(key)
            .unwrap_or_else(|e| panic!("Failed to delete db record: {}", e));
    }

    let shutdown = Arc::new(AtomicBool::new(false));
    let forwarding_thread = actix_rt::spawn({
        let db = db.clone();
        let tx = tx.clone();
        let client = client.clone();
        let shutdown = shutdown.clone();
        async move {
            let timeout = Duration::from_secs(3);
            loop {
                match actix_rt::time::timeout(timeout, rx.recv()).await {
                    Ok(res) => {
                        if let Ok(msg) = res {
                            match client.confirm_donation(&msg.1).await {
                                Ok(_) => db.delete(msg.0).unwrap_or_else(|e| {
                                    panic!("Failed to delete db record: {}", e)
                                }),
                                Err(e) => {
                                    // Communications issues? - Wait and retry.
                                    error!("Failed to process receipt: {}", e.to_string());
                                    actix_rt::time::sleep(Duration::from_secs(
                                        WAIT_SECONDS_BEFORE_RESEND,
                                    ))
                                    .await;
                                    tx.send(msg)
                                        .await
                                        .unwrap_or_else(|e| panic!("TX Error: {}", e));
                                }
                            }
                        }
                    }
                    Err(_) => {
                        if shutdown.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            }
        }
    });

    info!("Listening to {}...", config.listen_address);
    let bind = format!("{}", config.listen_address);

    HttpServer::new(move || {
        App::new()
            .wrap(Cors::default())
            .app_data(web::Data::new(tx.clone()))
            .app_data(web::Data::new(client.clone()))
            .app_data(db.clone())
            .service(web::resource("/ping").route(web::get().to(test_ping_handler)))
            .service(web::resource("/paypal").route(web::post().to(paypal_handler)))
    })
    .workers(config.num_threads)
    .bind(&bind)?
    .run()
    .await
    .unwrap();

    shutdown.store(true, Ordering::Relaxed);
    forwarding_thread.await.unwrap();

    Ok(())
}
