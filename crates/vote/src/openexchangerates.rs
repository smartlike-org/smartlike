use crate::{Configuration, CurrencyExchangeRatesUpdate};
use std::collections::HashMap;
use std::string::ToString;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Rates {
    pub disclaimer: String,
    pub license: String,
    pub timestamp: u32,
    pub base: String,
    pub rates: HashMap<String, f64>,
}
pub async fn download(config: &Configuration) -> anyhow::Result<CurrencyExchangeRatesUpdate> {
    println!("Querying {}", config.currency_exchange_query);

    let client = reqwest::Client::new();
    let resp = client
        .get(&config.currency_exchange_query)
        .send()
        .await
        .map_err(|err| anyhow::anyhow!("HTTP GET error: {}", err.to_string()))?
        .text()
        .await
        .map_err(|err| anyhow::anyhow!("Send error: {}", err.to_string()))?;

    parse(config.currency_exchange_source.clone(), &resp)
}

fn parse(source: String, resp: &str) -> anyhow::Result<CurrencyExchangeRatesUpdate> {
    let res: Result<Rates, String> = serde_json::from_str(&resp)
        .map_err(|err| anyhow::anyhow!("Parse error: {} {}", err.to_string(), resp).to_string());
    match res {
        Ok(r) => {
            return Ok(CurrencyExchangeRatesUpdate {
                source: source,
                base: r.base,
                ts: r.timestamp,
                rates: r.rates,
            });
        }
        Err(e) => {
            return Err(anyhow::anyhow!("{}", e.to_string()));
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
            parse("openexchangerates.org".to_string(), &resp).is_ok(),
            true
        );
    }
}
