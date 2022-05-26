use blake2::{Blake2b, Digest};
use ed25519_dalek::{ExpandedSecretKey, Keypair, PublicKey, SecretKey};
use rand::Rng;
use std::collections::HashMap;
use std::string::ToString;
use std::time::{SystemTime, UNIX_EPOCH};

/// Donation receipt.
///
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

/// Currency exchange rate parameters.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CurrencyExchangeRatesUpdate {
    pub source: String,
    pub base: String,
    pub ts: u32,
    pub rates: HashMap<String, f64>,
}

/// Apub message.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApubMessage {
    pub key_id: String,
    pub headers: String,
    pub algorithm: String,
    pub digest: String,
    pub signature: String,
    pub payload: String,
    pub ts: u32,
}

/// Like message.
///
#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Like {
    pub platform: String,
    pub id: String,
    pub target: String,
    pub amount: f64,
    pub currency: String,
}

/// A specialized Response type for Smartlike RPC.
///
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Response<T> {
    pub status: String,
    pub data: T,
}
/// Provides lightweight access to Smartlike RPC.
///
pub struct Client {
    pub account: String,
    pub keys: Keypair,
    pub network_address: String,
    pub http_client: reqwest::Client,
}

impl Clone for Client {
    fn clone(&self) -> Self {
        Client {
            account: self.account.clone(),
            keys: Keypair::from_bytes(&self.keys.to_bytes()).unwrap(),
            network_address: self.network_address.clone(),
            http_client: reqwest::Client::builder().use_rustls_tls().build().unwrap(),
        }
    }
}

impl Client {
    pub fn new(account: String, secret: String, network_address: String) -> Client {
        let mut seed: [u8; 32] = Default::default();
        seed.copy_from_slice(&Blake2b::digest(secret.as_bytes())[..32]);
        let secret = SecretKey::from_bytes(&seed).unwrap();
        let public: PublicKey = (&secret).into();

        let mut pair = vec![];
        pair.extend_from_slice(&seed);
        pair.extend_from_slice(public.as_bytes());

        Client {
            account,
            keys: Keypair::from_bytes(&pair).unwrap(),
            network_address,
            http_client: reqwest::Client::builder().use_rustls_tls().build().unwrap(),
        }
    }

    pub async fn confirm_donation(&self, receipt: &DonationReceipt) -> anyhow::Result<String> {
        let parameters = serde_json::to_string(&receipt)
            .map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;
        self.rpc("confirm_donation", &parameters, None).await
    }

    pub async fn update_exchange_rates(
        &self,
        update: &CurrencyExchangeRatesUpdate,
    ) -> anyhow::Result<String> {
        let parameters = serde_json::to_string(&update)
            .map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;
        self.rpc("update_exchange_rates", &parameters, None).await
    }

    pub async fn relay_apub(&self, receipt: &ApubMessage) -> anyhow::Result<String> {
        let parameters = serde_json::to_string(&receipt)
            .map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;
        self.rpc("relay_apub", &parameters, None).await
    }

    pub async fn forward_like(&self, like: &Like) -> anyhow::Result<String> {
        let parameters = serde_json::to_string(&like)
            .map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;
        self.rpc("forward_like", &parameters, None).await
    }

    pub fn sign(&self, message: &str) -> String {
        let expanded: ExpandedSecretKey = (&self.keys.secret).into();
        let sig = expanded.sign(message.as_bytes(), &self.keys.public);
        let strs: Vec<String> = sig
            .to_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        strs.join("")
    }

    async fn rpc(&self, method: &str, parameters: &str, id: Option<u64>) -> anyhow::Result<String> {
        let now = SystemTime::now();
        let ts: i32 = now
            .duration_since(UNIX_EPOCH)
            .map_err(|err| anyhow::anyhow!("Failed get timestamp: {}", err.to_string()))?
            .as_secs() as i32;
        let tx = json!({
            "kind": method,
            "ts": ts,
            "data": parameters,
        });
        let msg = serde_json::to_string(&tx)
            .map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;

        let rpc_id = match id {
            Some(v) => v,
            None => rand::thread_rng().gen::<u64>(),
        };

        let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "id": rpc_id,
        "params": {
            "signed_message": {
                "sender": self.account,
                "signature": self.sign(&msg),
                "data": msg,
              },
            }
        });

        let resp =
            self.http_client
                .post(&self.network_address)
                .body(serde_json::to_string(&body).map_err(|err| {
                    anyhow::anyhow!("Failed to serialize body: {}", err.to_string())
                })?)
                .send()
                .await
                .map_err(|err| anyhow::anyhow!("Send error: {}", err.to_string()))?;

        if resp.status() == 200 {
            let text = resp
                .text()
                .await
                .map_err(|err| anyhow::anyhow!("Get text error: {}", err.to_string()))?;

            let r: Response<String> = serde_json::from_str(&text)
                .map_err(|err| anyhow::anyhow!("Parse error: {} {}", err.to_string(), text))?;
            Ok(r.status)
        } else {
            Err(anyhow::anyhow!("HTTP response code: {}", resp.status()))
        }
    }
}
