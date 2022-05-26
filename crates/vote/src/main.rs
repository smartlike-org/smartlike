extern crate clap;
extern crate serde_json;
extern crate toml;
#[macro_use]
extern crate serde;

use smartlike_embed_lib::client::Client;
use std::{fs::File, io::prelude::*};

mod openexchangerates;

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Configuration {
    pub network_address: String,
    pub smartlike_account: String,
    pub smartlike_key: String,
    pub currency_exchange_source: String,
    pub currency_exchange_query: String,
}


async fn fetch_exchange_rates(client: &Client, config: &Configuration) -> anyhow::Result<()> {
    if config.currency_exchange_source == "openexchangerates.org" {
        let rates = openexchangerates::download(&config).await?;
        client.update_exchange_rates(&rates).await?;
    } else {
    }
    Ok(())
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

    let client = Client::new(
        config.smartlike_account.clone(),
        config.smartlike_key.clone(),
        config.network_address.clone(),
    );

    match fetch_exchange_rates(&client, &config).await {
        Ok(_) => {}
        Err(e) => println!("Error: {}", e.to_string())
    }
    Ok(())
}
