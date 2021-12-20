extern crate clap;
extern crate serde_json;
extern crate toml;
extern crate url;
#[macro_use]
extern crate serde;
extern crate rocksdb;

mod paypal;

use actix_cors::Cors;
use actix_web::{web, App, HttpResponse, HttpServer};
use rocksdb::{DBWithThreadMode, IteratorMode, MultiThreaded};
use serde_json::json;
use smartlike_embed_lib::client::Client;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::{fs::File, io::prelude::*, thread, time::Duration};

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Configuration {
    pub listen_address: String,
    pub num_threads: usize,
    pub network_address: String,
    pub smartlike_account: String,
    pub smartlike_key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DonationReceipt {
    pub donor: String,
    pub recipient: String,
    pub channel_id: String,
    pub alias: String,
    pub id: String,
    pub address: String,
    pub processor: String,
    pub amount: f64,
    pub currency: String,
    pub target_currency: String,
    pub ts: u32,
}

async fn paypal_handler(
    text: String,
    tx: web::Data<Sender<(String, String)>>,
    db: web::Data<DBWithThreadMode<MultiThreaded>>,
) -> actix_web::Result<HttpResponse> {
    match web::Query::from_query(&text) {
        Ok(q) => {
            match paypal::parse(text, q).await {
                Ok(receipt) => {
                    // Store the call until it's successfully processed.
                    let msg = serde_json::to_string(&receipt)?;
                    match db.put(receipt.id.clone(), msg.clone()) {
                        Ok(_) => {
                            // Do the forward in another thread to return IPN faster.
                            match tx.send((receipt.id, msg)) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("TX Error: {}", e);
                                }
                            };
                            Ok(HttpResponse::Ok()
                                .content_type("text/plain")
                                .body("".to_string()))
                        }
                        Err(e) => {
                            println!("DB error: {}", e);
                            Ok(HttpResponse::InternalServerError()
                                .content_type("text/plain")
                                .body("".to_string()))
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {}", e);
                    Ok(HttpResponse::Ok().content_type("text/plain").body("Error"))
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
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
        println!("{:?}", token);
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
    let shared_node = web::Data::new(client.clone());

    let (tx, rx) = channel::<(String, String)>();

    // Load pending receipts from previous runs and retry them.
    let iter = db.iterator(IteratorMode::Start);
    for (key, value) in iter {
        println!("Found pending request {:?} {:?}", key, value);
        match (
            String::from_utf8(key.to_vec()),
            String::from_utf8(value.to_vec()),
        ) {
            (Ok(k), Ok(v)) => {
                match tx.send((k, v)) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("TX Error: {}", e);
                    }
                };
            }
            _ => {
                println!("Failed to parse pending receipt. Rejecting.");
                match db.delete(key) {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Failed to delete db record: {}", e);
                    }
                }
            }
        }
    }

    let db_out = db.clone();
    let tx_out = tx.clone();
    actix_rt::spawn(async move {
        // todo: scale the number of threads
        let timeout = Duration::from_secs(10);

        loop {
            match rx.recv_timeout(timeout) {
                Ok(msg) => {
                    match client.rpc("confirm_donation", &msg.1, None).await {
                        Ok(_) => match db_out.delete(msg.0) {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Failed to delete db record: {}", e);
                            }
                        },
                        Err(e) => {
                            println!("Failed to process receipt: {}", e.to_string());
                            // Communications issues? - Wait and retry.
                            thread::sleep(Duration::from_secs(5));
                            match tx_out.send(msg) {
                                Ok(_) => {}
                                Err(e) => {
                                    println!("TX Error: {}", e);
                                }
                            };
                        }
                    }
                }
                Err(_) => {}
            }
        }
    });

    println!("Listening to {}...", config.listen_address);
    let bind = format!("{}", config.listen_address);

    HttpServer::new(move || {
        App::new()
            .wrap(Cors::default())
            .app_data(web::Data::new(tx.clone()))
            .app_data(shared_node.clone())
            .app_data(db.clone())
            .service(web::resource("/ping").route(web::get().to(test_ping_handler)))
            .service(web::resource("/paypal").route(web::post().to(paypal_handler)))
    })
    .workers(config.num_threads)
    .bind(&bind)?
    .run()
    .await
    .unwrap();

    Ok(())
}
