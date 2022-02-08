use blake2::{Blake2b, Digest};
use ed25519_dalek::{ExpandedSecretKey, Keypair, PublicKey, SecretKey};
use rand::Rng;
use std::string::ToString;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::Response;

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

    pub async fn rpc(&self, method: &str, parameters: &str, id: Option<u64>) -> anyhow::Result<()>
    {
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
        let msg = serde_json::to_string(&tx).map_err(|err| anyhow::anyhow!("Failed to serialize message: {}", err.to_string()))?;

        let rpc_id = match id { Some(v) => v, None => { rand::thread_rng().gen::<u64>()} };

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

        println!("Sending {} to {}", serde_json::to_string(&body).map_err(|err| anyhow::anyhow!("Failed to serialize body: {}", err.to_string()))?, self.network_address);

        let resp = self
            .http_client
            .post(&self.network_address)
            .body(serde_json::to_string(&body).map_err(|err| anyhow::anyhow!("Failed to serialize body: {}", err.to_string()))?)
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
            if r.status == "ok" {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Error: {}", r.status))
            }
        } else {
            Err(anyhow::anyhow!("HTTP response code: {}", resp.status()))
        }
    }
}
