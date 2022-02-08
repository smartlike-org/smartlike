use crate::{util, Context};
use actix_web::web;
use fasthash::city::hash64;
use lru::LruCache;
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Public};
use regex::Regex;
use reqwest::header;
use rocksdb::{DBWithThreadMode, IteratorMode, MultiThreaded};
use serde::Serialize;
use serde_json::json;
use smartlike_embed_lib::client::Client;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

lazy_static! {
    static ref RE_SIG: Regex =
        Regex::new(r"Smartlike:\s?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
            .unwrap();
    static ref RE_UUID: Regex =
        Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Signature {
    pub r#type: String,
    pub creator: String,
    pub created: String,
    pub signature_value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub key_id: String,
    pub headers: String,
    pub algorithm: String,
    pub digest: String,
    pub signature: String,
    pub payload: String,
    pub ts: u32,
}

impl Message {
    pub fn verify_https_signature(&self, public_key: &PKey<Public>) -> Result<bool, anyhow::Error> {
        let digest = openssl::hash::hash(
            openssl::hash::MessageDigest::sha256(),
            self.payload.as_bytes(),
        )?;
        let mut digest_header = "SHA-256=".to_owned();
        base64::encode_config_buf(digest, base64::STANDARD, &mut digest_header);
        if self.digest != digest_header {
            println!("{}\n{}", self.digest, digest_header);
            return Ok(false);
        }
        let sig = base64::decode(self.signature.clone())?;
        Ok(util::verify(
            public_key,
            MessageDigest::sha256(),
            self.headers.as_bytes(),
            &sig,
        )?)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ActorState {
    NoAccount,
    AccountPublished,
    Error,
    Ignore,
    Relay,
    BlackList,
}

#[derive(Clone)]
pub struct Actor {
    pub state: ActorState,
    pub public_key: Option<PKey<Public>>,
    pub last_checked: Instant,
}

impl Actor {
    pub fn _get_public_key(_id: &str) -> Result<PKey<Public>, anyhow::Error> {
        Ok(PKey::public_key_from_pem("".to_string().as_bytes())?)
    }
}

#[derive(Clone)]
pub struct Dispatcher {
    pub relay_channels: Vec<Sender<Message>>,
    pub db: Arc<DBWithThreadMode<MultiThreaded>>,
}

impl Dispatcher {
    pub fn send(&self, message: Message) -> Result<(), anyhow::Error> {
        // The same ids are dispatched to the same relay channels to utilize their caches.
        let ch = (hash64(&message.key_id) % self.relay_channels.len() as u64) as usize;

        // make a backup copy
        let msg = serde_json::to_string(&message)?;
        match self.db.put(message.key_id.clone(), msg.clone()) {
            Ok(_) => {}
            Err(e) => println!("Failed to store message to db: {}", e.to_string()),
        }

        match self.relay_channels[ch].send(message) {
            Ok(_) => {}
            Err(e) => {
                println!("TX Error: {}", e);
            }
        };

        Ok(())
    }

    pub fn recover_queue(&self) -> Result<(), anyhow::Error> {
        // Load queue from previous runs and retry.
        let iter = self.db.iterator(IteratorMode::Start);
        for (key, value) in iter {
            println!("Found pending request {:?}", key);
            match String::from_utf8(value.to_vec()) {
                Ok(v) => {
                    let message_res: Result<Message, _> = serde_json::from_str(&v);
                    if let Ok(message) = message_res {
                        match self.send(message) {
                            Ok(_) => {}
                            Err(e) => {
                                println!("TX Error: {}", e);
                            }
                        };
                    } else {
                        println!("Failed to parse pending receipt. Rejecting.");
                        match self.db.delete(key) {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Failed to delete db record: {}", e);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

pub struct Relay {
    pub context: web::Data<Context>,
    pub actors: LruCache<String, Actor>,
    pub http_client: reqwest::Client,
    pub smartlike_client: Arc<Client>,
    pub retry_check_account_period: Duration,
    pub verify_account_period: Duration,
}

impl Relay {
    pub fn create(
        context: web::Data<Context>,
        smartlike_client: Arc<Client>,
    ) -> Result<Relay, anyhow::Error> {
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/activity+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("fediverse-smartlike-relay"),
        );

        Ok(Relay {
            actors: LruCache::new(context.config.max_actor_cache_size),
            context,
            http_client: reqwest::Client::builder()
                .use_rustls_tls()
                .default_headers(headers)
                .build()?,
            smartlike_client,
            retry_check_account_period: Duration::from_secs(3600),
            verify_account_period: Duration::from_secs(5 * 24 * 3600),
        })
    }

    pub async fn get_actor(&mut self, id: &String, account_required: bool) -> Option<Actor> {
        println!("get_author: {}", id);
        if let Some(v) = self.actors.get(id) {
            match v.state {
                ActorState::NoAccount => {
                    if !account_required {
                        if v.public_key.is_some() {
                            return Some(v.clone());
                        } else if v.last_checked.elapsed() < self.retry_check_account_period {
                            return None;
                        }
                    }
                }
                ActorState::AccountPublished => {
                    if !account_required {
                        if v.public_key.is_some() {
                            return Some(v.clone());
                        } else if v.last_checked.elapsed() < self.retry_check_account_period {
                            return None;
                        }
                    }
                }
                ActorState::Error => {
                    if v.last_checked.elapsed() < self.retry_check_account_period {
                        return None;
                    }
                }
                _ => {
                    println!("unhandeled actor state");
                    return None;
                }
            }
        }

        // fetch public key

        let a = Actor {
            state: ActorState::Error,
            public_key: None,
            last_checked: Instant::now(),
        };
        self.actors.put(id.to_string(), a);

        let address = match self.context.config.protocol == "http" {
            true => str::replace(id, "https:", "http:"),
            false => id.to_string(),
        };

        let mut response = None;
        match self.http_client.get(&address).send().await {
            Ok(resp) => {
                if resp.status() == 200 {
                    match resp.text().await {
                        Ok(text) => {
                            response = Some(text);
                        }
                        Err(e) => {
                            println!("Failed to get response: {}", e.to_string());
                        }
                    }
                } else {
                    println!("HTTP response: {}", resp.status());
                }
            }
            Err(e) => {
                println!("HTTP query failed: {}", e.to_string());
            }
        }

        if let Some(text) = response {
            let j_res: Result<serde_json::Value, _> = serde_json::from_str(&text);
            let mut account = None;
            let mut public_key = None;
            if let Ok(j) = j_res {
                // Smartlike account published?
                if let Some(s) = j.get("summary") {
                    if let Some(summary) = s.as_str() {
                        if let Some(sig_match) = RE_SIG.find(summary) {
                            if let Some(account_match) = RE_UUID.find(sig_match.as_str()) {
                                account = Some(account_match.as_str().to_string());
                                println!("Found account for {}: {}", id, account_match.as_str());
                            }
                        }
                    }
                }

                if let Some(p) = j.get("publicKey") {
                    if let Some(k) = p.get("publicKeyPem") {
                        match k.as_str() {
                            Some(pk_str) => {
                                if let Ok(pk) = PKey::public_key_from_pem(pk_str.as_bytes()) {
                                    println!("Found public key for {}: {}", id, pk_str);
                                    public_key = Some(pk);
                                }
                            }
                            None => {
                                println!("failed to convert public key");
                            }
                        }
                    }
                }
            }

            let a = Actor {
                state: match public_key.is_some() {
                    true => match account.is_some() {
                        true => ActorState::AccountPublished,
                        false => ActorState::NoAccount,
                    },
                    false => ActorState::Error,
                },
                public_key: public_key,
                last_checked: Instant::now(),
            };
            self.actors.put(id.to_string(), a.clone()); // todo: optimize

            if a.public_key.is_some() && (!account_required || account.is_some()) {
                Some(a)
            } else {
                None
            }
        } else {
            println!("Failed to get public key from response");
            None
        }
    }

    pub async fn verify_message(
        &mut self,
        msg: &Message,
        body_value: &mut serde_json::Value,
        account_required: bool,
    ) -> Result<(), anyhow::Error> {
        println!("http signer: {}", msg.key_id);
        if let Some(actor_data) = self.get_actor(&msg.key_id, false).await {
            println!("http signer found");
            if let Some(pk) = actor_data.public_key {
                match msg.verify_https_signature(&pk) {
                    Ok(res) => {
                        if res {
                            println!("HTTP signature verified.");
                        } else {
                            return Err(anyhow::anyhow!("Failed to validate http signature"));
                        }
                    }
                    Err(e) => {
                        return Err(anyhow::anyhow!(
                            "Failed to validate http signature: {}",
                            e.to_string()
                        ));
                    }
                }
            }
        } else {
            return Err(anyhow::anyhow!("failed to get author: {}", msg.key_id));
        }

        let body_object = body_value.as_object_mut().unwrap();
        let mut signature_value = body_object
            .get("signature")
            .ok_or(anyhow::anyhow!("failed to parse RSA signature"))?
            .clone();
        let signature = signature_value.as_object_mut().unwrap();
        body_object.remove("signature");
        let body_without_signature = serde_json::to_string(&body_object)?;
        let document_hash = util::normalize_hash(&body_without_signature).await?;

        let creator = signature
            .get("creator")
            .ok_or(anyhow::anyhow!("failed to parse creator"))?
            .as_str()
            .ok_or(anyhow::anyhow!("failed to parse creator"))?
            .to_string();

        if let Some(actor_data) = self.get_actor(&creator, account_required).await {
            println!("RSA signer found");
            if let Some(pk) = actor_data.public_key {
                let signature_value = signature
                    .get("signatureValue")
                    .ok_or(anyhow::anyhow!("failed to parse RSA signature"))?
                    .as_str()
                    .ok_or(anyhow::anyhow!("failed to parse RSA signature"))?;

                let decoded_sig = base64::decode(signature_value)?;

                signature.insert(
                    "@context".to_string(),
                    json!([
                        "https://w3id.org/security/v1",
                        { "RsaSignature2017": "https://w3id.org/security#RsaSignature2017" }
                        ]
                    ),
                );

                signature.remove("type");
                signature.remove("id");
                signature.remove("signatureValue");

                let options_hash =
                    util::normalize_hash(&serde_json::to_string(&signature)?).await?;
                let to_be_signed = options_hash + &document_hash;

                let verified = util::verify(
                    &pk,
                    MessageDigest::sha256(),
                    to_be_signed.as_bytes(),
                    &decoded_sig,
                )?;

                if !verified {
                    println!(
                        "Failed to verify RSA signatures {} - {}",
                        document_hash,
                        serde_json::to_string(&signature)?,
                    );
                } else {
                    println!("Succeeded to verify RSA signatures");
                    return Ok(());
                }
            }
        }
        Err(anyhow::anyhow!("Failed to verify RSA signatures"))
    }
}

pub async fn run_thread(
    _channel: usize,
    mut relay: Relay,
    rx: Receiver<Message>,
    db: Arc<DBWithThreadMode<MultiThreaded>>,
) {
    let timeout = Duration::from_secs(10);

    loop {
        match rx.recv_timeout(timeout) {
            Ok(msg) => {
                let payload: Result<serde_json::Value, _> = serde_json::from_str(&msg.payload);
                if let Ok(mut j) = payload {
                    let (_id, t) = match (j.get("id"), j.get("type")) {
                        (Some(id), Some(t)) => (id, t),
                        (_, _) => (&serde_json::Value::Null, &serde_json::Value::Null),
                    };

                    if t == "Like" {
                        //println!("like: {} -> {}", a, o);
                        if relay.verify_message(&msg, &mut j, true).await.is_ok() {
                            loop {
                                match relay
                                    .smartlike_client
                                    .rpc("relay_apub", &msg.payload, None)
                                    .await
                                {
                                    Ok(_) => {
                                        match db.delete(msg.key_id) {
                                            Ok(_) => {}
                                            Err(e) => {
                                                println!("Failed to delete db record: {}", e);
                                            }
                                        }
                                        break;
                                    }
                                    Err(e) => {
                                        println!(
                                            "Failed to send message: {}. Retry in 600 sec.",
                                            e.to_string()
                                        );
                                        thread::sleep(Duration::from_secs(600));
                                    }
                                }
                            }
                        }
                    } else if t == "Follow" {
                        println!("follow");
                        if relay.verify_message(&msg, &mut j, false).await.is_ok() {}
                    }
                }
            }
            Err(_) => {}
        }
    }
}
