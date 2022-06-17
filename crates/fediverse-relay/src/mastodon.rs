use crate::Context;
use crate::{relay, util};
use actix_web::web;
use actix_web::HttpRequest;
use anyhow::anyhow;
use serde_json::json;
use tracing::trace;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Follow {
    #[serde(rename = "@context")]
    pub context: String,
    pub id: String,
    pub r#type: String,
    pub actor: String,
    pub object: String,
}

pub async fn handle_follow(
    j: &serde_json::Value,
    dispatcher: web::Data<relay::Dispatcher>,
    context: web::Data<Context>,
) -> Result<(), anyhow::Error> {
    trace!("follow mastodon");

    match (
        j.get("@context").and_then(|v| v.as_str()),
        j.get("id").and_then(|v| v.as_str()),
        j.get("actor").and_then(|v| v.as_str()),
        j.get("object"),
    ) {
        (Some(ctx), Some(id), Some(actor), Some(object)) => {
            if ctx != "https://www.w3.org/ns/activitystreams" {
                return Err(anyhow!("unkonwn message type"));
            }

            let remote_instance = util::get_domain(&actor)?;
            let follower_uuid = uuid::Uuid::new_v4();

            let accept = json!({
                "@context": "https://www.w3.org/ns/activitystreams",
                "type": "Accept",
                "to": [actor],
                "actor": format!("https://{}/actor", context.config.instance),
                "object": {
                    "type": "Follow",
                    "id": id,
                    "object": object,
                    "actor": actor,
                },
                "id": format!("https://{}/activities/{}", context.config.instance, follower_uuid)
            });

            dispatcher
                .respond(relay::Reply {
                    instance: remote_instance,
                    path: "/inbox".to_string(),
                    message: accept,
                    key_id: format!("https://{}/actor#main-key", context.config.instance),
                    sign_body: false,
                })
                .await?;

            Ok(())
        }
        _ => Err(anyhow!("failed to parse body")),
    }
}

pub async fn handle_boost(
    req: HttpRequest,
    j: &serde_json::Value,
    payload: String,
    dispatcher: web::Data<relay::Dispatcher>,
) -> Result<(), anyhow::Error> {
    trace!("boost mastodon");

    match (
        j.get("@context").and_then(|v| v.as_str()),
        j.get("id").and_then(|v| v.as_str()),
        j.get("actor").and_then(|v| v.as_str()),
        j.get("object"),
    ) {
        (Some(ctx), Some(_id), Some(actor), Some(object)) => {
            if ctx != "https://www.w3.org/ns/activitystreams" {
                return Err(anyhow!("unkonwn message type"));
            }

            trace!("boost: {} -> {}", actor, object);
            match util::prepare_message(req, "/", payload) {
                Ok(msg) => dispatcher.send(msg).await,
                Err(e) => Err(anyhow!("Failed to prepare message: {}", e)),
            }
        }
        _ => Err(anyhow!("failed to parse body")),
    }
}
