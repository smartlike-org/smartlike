use dashmap::DashMap;
use openssl::pkey::{PKey, Private, Public};
use reqwest::header;
use rocksdb::{DBWithThreadMode, IteratorMode, MultiThreaded};
use std::collections::HashMap;
use std::sync::Arc;
use std::{fs, io};
use std::{fs::File, io::prelude::*};
use tracing::{error, trace};

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Configuration {
    pub listen_address: String,
    pub num_web_server_threads: usize,
    pub num_relay_threads: usize,
    pub network_address: String,
    pub smartlike_account: String,
    pub smartlike_key: String,

    pub name: String,
    pub summary: String,
    pub public_key: String,
    pub private_key: String,
    pub instance: String,
    pub max_actor_cache_size: usize,
    pub protocol: String,
    pub log_target: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Instance {
    pub id: String,
}

pub struct Context {
    pub config: Configuration,
    pub node: String,
    pub node_meta: String,
    pub actor: String,
    pub public_key: PKey<Public>,
    pub private_key: PKey<Private>,
    pub replies: HashMap<String, String>,
    pub db: Arc<DBWithThreadMode<MultiThreaded>>,
    pub following: DashMap<String, Instance>,
    pub http_client: reqwest::Client,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum Platform {
    Unknown,
    General,
    Mastodon,
    PeerTube,
}

impl Context {
    pub fn create(config: &str) -> Result<Context, anyhow::Error> {
        let mut f = File::open(config).unwrap();
        let mut contents = String::new();
        f.read_to_string(&mut contents).unwrap();
        let config = toml::from_str::<Configuration>(&contents)
            .map_err(|e| anyhow::anyhow!("Error loading configuration: {}", e.to_string()))?;
        let mut replies: HashMap<String, String> = HashMap::new();
        let templates = fs::read_dir("./templates")?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;
        for t in templates {
            match t.into_os_string().into_string() {
                Ok(v) => {
                    let response = fs::read_to_string(v.clone())?;
                    let parts: Vec<&str> = v.split('/').collect();
                    if parts.len() > 1 {
                        if v.ends_with(".json") {
                            let parsed = json::parse(&response).map_err(|e| {
                                anyhow::anyhow!(
                                    "Failed to parse template {}. {}",
                                    parts[parts.len() - 1],
                                    e.to_string()
                                )
                            })?;
                            let stringified = json::stringify(parsed);
                            let j = str::replace(
                                &str::replace(&stringified, "{INSTANCE}", &config.instance),
                                "{PUBLIC_KEY}",
                                &str::replace(&config.public_key, "\n", "\\n"),
                            )
                            .to_string();

                            replies.insert(parts[parts.len() - 1].to_string(), j);
                        } else {
                            replies.insert(parts[parts.len() - 1].to_string(), response);
                        }
                    }
                }
                Err(_) => {
                    return Err(anyhow::anyhow!("Failed to read templates."));
                }
            }
        }
        let mut headers = header::HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/activity+json"),
        );
        headers.insert(
            header::USER_AGENT,
            header::HeaderValue::from_static("fediverse-smartlike-relay"),
        );

        let db = Arc::new(DBWithThreadMode::<MultiThreaded>::open_default("./db/context").unwrap());
        let iter = db.iterator(IteratorMode::Start);
        let following = DashMap::new();
        for (key, value) in iter {
            trace!("Found server {:?} {:?}", key, value);
            match (
                String::from_utf8(key.to_vec()),
                String::from_utf8(value.to_vec()),
            ) {
                (Ok(k), Ok(v)) => {
                    let i: Instance = serde_json::from_str(&v)?;
                    following.insert(k, i);
                }
                _ => {
                    error!("Failed to parse server. Rejecting.");
                    match db.delete(key) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Failed to delete db record: {}", e);
                        }
                    }
                }
            }
        }

        Ok(Context {
            actor: "".to_string(),
            node: "".to_string(),
            node_meta: "".to_string(),
            public_key: PKey::public_key_from_pem(config.public_key.as_bytes())?,
            private_key: PKey::private_key_from_pem(config.private_key.as_bytes())?,
            replies,
            db,
            following,
            config,
            http_client: reqwest::Client::builder()
                .use_rustls_tls()
                .default_headers(headers)
                .build()?,
        })
    }

    pub fn _add_instance(&mut self, instance: &str, id: &str) -> Result<(), anyhow::Error> {
        if let Some(_v) = self.following.get(instance) {
            Ok(())
        } else {
            let new = Instance { id: id.to_string() };
            let msg = serde_json::to_string(&new)?;
            match self.db.put(instance.to_string(), msg.clone()) {
                Ok(_) => {
                    self.following.insert(instance.to_string(), new);
                    Ok(())
                }
                Err(e) => {
                    error!("DB error: {}", e);
                    Err(anyhow::anyhow!(
                        "Error loading configuration: {}",
                        e.to_string()
                    ))
                }
            }
        }
    }
}
