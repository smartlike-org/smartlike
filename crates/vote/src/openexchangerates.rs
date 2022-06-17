use crate::{Configuration};
use std::collections::HashMap;
use std::string::ToString;
use smartlike_embed_lib::client::CurrencyExchangeRatesUpdate;
use anyhow::anyhow;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rates {
    disclaimer: String,
    license: String,
    timestamp: u32,
    base: String,
    rates: HashMap<String, f64>,
}
pub async fn download(config: &Configuration) -> anyhow::Result<CurrencyExchangeRatesUpdate> {
    trace!("Querying {}", config.currency_exchange_query);

    let client = reqwest::Client::new();
    let resp = client
        .get(&config.currency_exchange_query)
        .send()
        .await
        .map_err(|err| anyhow!("HTTP GET error: {}", err))?
        .text()
        .await
        .map_err(|err| anyhow!("Failed to get request body: {}", err))?;

    parse(&config.currency_exchange_source, &resp)
}

fn parse(source: &str, resp: &str) -> anyhow::Result<CurrencyExchangeRatesUpdate> {
    let res: Result<Rates, String> = serde_json::from_str(&resp)
        .map_err(|err| anyhow!("Parse error: {} {}", err, resp).to_string());
    match res {
        Ok(r) => {
            return Ok(CurrencyExchangeRatesUpdate {
                source: source.to_string(),
                base: r.base,
                ts: r.timestamp,
                rates: r.rates,
            });
        }
        Err(e) => {
            return Err(anyhow!("{}", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openexchange_parsing() {
        let resp = std::fs::read_to_string("./test/openexchangerates.json").unwrap();
        assert_eq!(
            parse("openexchangerates.org", &resp).is_ok(),
            true
        );
    }
}
