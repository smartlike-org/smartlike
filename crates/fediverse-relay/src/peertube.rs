use crate::util;
use crate::Context;
use anyhow::anyhow;
use tracing::trace;
use util::sign_and_send;

pub async fn follow(
    instance: &str,
    platform: &str,
    context: &Context,
) -> Result<(), anyhow::Error> {
    trace!("follow");
    let key_id = match platform {
        "peertube" => format!("https://{}/accounts/peertube", context.config.instance),
        _ => {
            return Err(anyhow!("unkonwn platform"));
        }
    };
    if let Some(v) = context
        .replies
        .get("POST_%2Faccount%2Fpeertube%2Finbox_follow.json")
    {
        let mut j: serde_json::Value = serde_json::from_str(v)?;
        j["id"] = serde_json::Value::String(
            format!(
                "https://{}/accounts/peertube/follows/1",
                context.config.instance
            )
            .to_string(),
        );
        j["object"] = serde_json::Value::String(
            format!("https://{}/accounts/peertube", instance).to_string(),
        );
        sign_and_send(
            instance,
            "/accounts/peertube/inbox",
            context,
            &mut j,
            &key_id,
            true,
        )
        .await
    } else {
        Err(anyhow!("failed to construct message"))
    }
}
