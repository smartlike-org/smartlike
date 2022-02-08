use crate::{relay, Context};
use actix_web::HttpRequest;
use chrono::Utc;
use futures::FutureExt;
use openssl::{
    hash::MessageDigest,
    pkey::{PKey, Private, Public},
    sha::sha256,
    sign::{Signer, Verifier},
};
use reqwest::header;
use serde_json::json;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn _verify_signature() -> Result<(), anyhow::Error> {
    Ok(())
}

pub fn _add_signature() -> Result<(), anyhow::Error> {
    Ok(())
}

pub async fn sign_and_send(
    instance: &str,
    path: &str,
    context: &Context,
    message: &mut serde_json::Value,
    key_id: &str,
    sign_body: bool,
) -> Result<(), anyhow::Error> {
    let address = format!("{}://{}{}", context.config.protocol, instance, path);
    let now = Utc::now();
    let http_date = now.format("%a, %d %b %Y %T GMT").to_string(); // Fri, 28 Jan 2022 10:44:17 GMT
    let sig_date = now.format("%FT%T%.3fZ").to_string(); // 2022-01-28T10:44:17.258Z

    if sign_body {
        let options = json!({
                "@context": [
                    "https://w3id.org/security/v1",
                    { "RsaSignature2017": "https://w3id.org/security#RsaSignature2017" }
                ],
                "created": sig_date,
                "creator": key_id,
        });

        let body_without_signature = serde_json::to_string(&message)?;
        let document_hash =
            normalize_hash(&body_without_signature).await?;

        let options_hash = normalize_hash(&serde_json::to_string(&options)?).await?;
        let to_be_signed = options_hash + &document_hash;

        let signature = base64::encode(
            &sign(&context.private_key, to_be_signed.as_bytes())
                .map_err(|_err| anyhow::anyhow!("Failed to encode signature"))?,
        );

        let sig = json!({
            "type": "RsaSignature2017",
            "creator": key_id,
            "created": sig_date,
            "signatureValue": serde_json::Value::String(signature),
        });
        message["signature"] = sig;
    }

    // HTTPS signature
    let body = serde_json::to_string(&message)?;
    let digest = openssl::hash::hash(openssl::hash::MessageDigest::sha256(), body.as_bytes())?;
    let mut digest_header = "SHA-256=".to_owned();
    base64::encode_config_buf(digest, base64::STANDARD, &mut digest_header);

    let mut headers = header::HeaderMap::new();
    headers.insert(header::DATE, header::HeaderValue::from_str(&http_date)?);
    headers.insert("Digest", header::HeaderValue::from_str(&digest_header)?);
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/json"),
    );

    let to_sign = format!(
        "(request-target): {} {}\nhost: {}\ndate: {}\ndigest: {}",
        http::Method::POST.as_str().to_lowercase(),
        path,
        instance,
        http_date,
        digest_header
    )
    .to_string();

    let sig_header = format!(
        "keyId=\"{}\",algorithm=\"rsa-sha256\",headers=\"(request-target) host date digest\",signature=\"{}\"",
        key_id, base64::encode(sign(&context.private_key, to_sign.as_bytes())?)
    ).to_string();

    println!("Signature: {}", sig_header);
    headers.insert("Signature", header::HeaderValue::from_str(&sig_header)?);

    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/activity+json"),
    );
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static("fediverse-smartlike-relay"),
    );

    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .default_headers(headers)
        .build()?;

    println!("Sending POST to {}\n{}", address, body);
    let resp = client
        .post(&address)
        .body(body)
        .send()
        .await
        .map_err(|err| anyhow::anyhow!("Send error: {}", err.to_string()))?;

    if resp.status() == 200 {
        Ok(())
    } else {
        Err(anyhow::anyhow!("HTTP response code: {}", resp.status()))
    }
}

pub fn hash(data: &str) -> String {
    let bytes = data.as_bytes();
    hex::encode(sha256(bytes))
}

pub fn sign(key: &PKey<Private>, src: &[u8]) -> Result<Vec<u8>, openssl::error::ErrorStack> {
    println!("signing {}", std::str::from_utf8(src).unwrap());
    let mut signer = Signer::new(MessageDigest::sha256(), key)?;
    signer.update(src)?;
    signer.sign_to_vec()
}

pub fn verify(
    key: &PKey<Public>,
    alg: openssl::hash::MessageDigest,
    src: &[u8],
    sig: &[u8],
) -> Result<bool, openssl::error::ErrorStack> {
    let mut verifier = Verifier::new(alg, key)?;
    verifier.update(src)?;
    verifier.verify(sig)
}

pub fn get_ts() -> anyhow::Result<u32> {
    let now = SystemTime::now();
    let ts: u32 = now
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("{}", e.to_string()))?
        .as_secs() as u32;
    Ok(ts)
}

fn parse_field<'a>(field: &'a str) -> anyhow::Result<(&'a str, &'a str)> {
    let idx = field.find('=').ok_or(anyhow::anyhow!(
        "failed to parse signature field '{}'",
        field
    ))?;
    let key = &field[..idx];
    let value = &field[(idx + 1)..];

    if value.starts_with('"') && value.ends_with('"') {
        Ok((key, &value[1..(value.len() - 1)]))
    } else {
        Ok((key, value))
    }
}

pub fn prepare_message(
    req: HttpRequest,
    path: &str,
    payload: String,
) -> anyhow::Result<relay::Message> {
    let h = req.head().headers();
    match (
        h.get("host"),
        h.get("date"),
        h.get("digest"),
        h.get("signature"),
    ) {
        (Some(host), Some(date), Some(digest), Some(sig)) => {
            // todo: check time drift

            let d = digest.to_str()?;
            let mut msg = relay::Message {
                key_id: "".to_string(),
                headers: format!(
                    "(request-target): {} {}\nhost: {}\ndate: {}\ndigest: {}",
                    req.method().as_str().to_lowercase(),
                    path,
                    host.to_str()?,
                    date.to_str()?,
                    d
                )
                .to_string(),
                algorithm: "".to_string(),
                digest: d.to_string(),
                signature: "".to_string(),
                payload,
                ts: get_ts()?,
            };

            for par in sig.to_str()?.split(',') {
                let (name, value) = parse_field(par)?;
                match name {
                    "keyId" => msg.key_id = value.to_string(),
                    "algorithm" => msg.algorithm = value.to_string(),
                    "signature" => msg.signature = value.to_string(),
                    _ => {}
                }
            }

            Ok(msg)
        }
        (_, _, _, _) => Err(anyhow::anyhow!("missing header")),
    }
}

pub const CONTEXT_IRIS: [&str; 2] = [
    "https://www.w3.org/ns/activitystreams",
    "https://w3id.org/security/v1",
];

lazy_static! {
    pub static ref CONTEXTS: HashMap<String, json_ld::RemoteDocument<json::JsonValue>> = {
        let mut contexts = HashMap::new();
        for c in CONTEXT_IRIS {
            let file_name = format!("./contexts/{}.jsonld", urlencoding::encode(c).to_string());
            println!("- {} - {}", c, file_name);
            let jsonld = std::fs::read_to_string(&file_name).unwrap();
            let doc = json::parse(&jsonld).unwrap();
            let iri = iref::Iri::new(c).unwrap();
            contexts.insert(c.to_string(), json_ld::RemoteDocument::new(doc, iri));
        }
        contexts
    };
}

pub struct ApubLoader;
impl json_ld::Loader for ApubLoader {
    type Document = json::JsonValue;
    fn load<'a>(
        &'a mut self,
        url: iref::Iri<'_>,
    ) -> futures::future::BoxFuture<
        'a,
        Result<json_ld::RemoteDocument<Self::Document>, json_ld::Error>,
    > {
        let url: iref::IriBuf = url.into();
        async move {
            match CONTEXTS.get(url.as_str()) {
                Some(d) => Ok(d.clone()),
                None => {
                    eprintln!("unknown context {}", url.as_str());
                    Err(json_ld::ErrorCode::LoadingDocumentFailed.into())
                }
            }
        }
        .boxed()
    }
}

pub async fn normalize_hash(j: &str) -> anyhow::Result<String> {
    let mut loader = ApubLoader;
    let normalized = ssi::jsonld::json_to_dataset(j, None, true, None, &mut loader)
        .await
        .and_then(|dataset| ssi::urdna2015::normalize(&dataset))
        .and_then(|dataset| dataset.to_nquads())?;
    Ok(hash(&normalized))
}
