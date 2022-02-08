extern crate clap;
extern crate serde_json;
extern crate toml;
extern crate url;
#[macro_use]
extern crate serde;
extern crate rocksdb;
#[macro_use]
extern crate lazy_static;
extern crate json_ld;
extern crate ssi;

mod context;
mod peertube;
mod relay;
mod routes;
mod util;

use actix_web::{middleware::Logger, web, App, HttpServer};
use context::Context;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use smartlike_embed_lib::client::Client;
use std::sync::mpsc::channel;
use std::sync::Arc;

#[actix_rt::main]
async fn main() -> anyhow::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();

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

    let config_file = matches.value_of("config").unwrap();

    let context = web::Data::new(Context::create(config_file)?);

    let db = Arc::new(DBWithThreadMode::<MultiThreaded>::open_default("./db/queue").unwrap());

    let smartlike_client = Arc::new(Client::new(
        context.config.smartlike_account.clone(),
        context.config.smartlike_key.clone(),
        context.config.network_address.clone(),
    ));

    let mut dispatcher = relay::Dispatcher {
        relay_channels: Vec::new(),
        db: db.clone(),
    };
    let mut relay_threads = vec![];
    for i in 0..context.config.num_relay_threads {
        let (tx, rx) = channel::<relay::Message>();
        dispatcher.relay_channels.push(tx);

        let relay = relay::Relay::create(context.clone(), smartlike_client.clone())?;

        relay_threads.push(actix_rt::spawn(relay::run_thread(i, relay, rx, db.clone())));
    }

    dispatcher.recover_queue()?;

    println!("Listening to {}...", context.config.listen_address);
    let bind = format!("{}", context.config.listen_address);
    let num_web_server_threads = context.config.num_web_server_threads;

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .app_data(web::Data::new(dispatcher.clone()))
            .app_data(context.clone())
            .service(web::resource("/inbox").route(web::post().to(routes::inbox)))
            .service(web::resource("/actor").route(web::get().to(routes::actor)))
            .service(web::resource("/nodeinfo/2.0.json").route(web::get().to(routes::nodeinfo)))
            .service(
                web::scope("/accounts")
                    .service(
                        web::resource("/{account_id}")
                            .route(web::get().to(routes::get_accounts))
                            .route(web::post().to(routes::post_accounts)),
                    )
                    .service(
                        web::resource("/{account_id}/{end_point}")
                            .route(web::get().to(routes::get_accounts))
                            .route(web::post().to(routes::post_accounts_endpoint)),
                    ),
            )
            .service(
                web::scope("/.well-known")
                    .service(web::resource("/nodeinfo").route(web::get().to(routes::nodeinfo_meta)))
                    .service(web::resource("/webfinger").route(web::get().to(routes::webfinger))),
            )
            .service(
                web::scope("/api")
                    .service(
                        web::resource("/follow/{platform}")
                            .route(web::post().to(routes::post_api_follow)),
                    )
                    .service(
                        web::resource("/test_relay")
                            .route(web::post().to(routes::post_api_test_relay)),
                    ),
            )
            .service(web::resource("/").route(web::get().to(routes::index)))
    })
    .workers(num_web_server_threads)
    .bind(&bind)?
    .run()
    .await
    .unwrap();

    Ok(())
}
