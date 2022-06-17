use crate::{
    context::{Context, Platform},
    mastodon, peertube, relay, util,
};
use actix_web::{http::Method, web, HttpRequest, HttpResponse};
use std::collections::HashMap;
use tracing::{error, trace, warn};

pub async fn nodeinfo(context: web::Data<Context>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/activity+json")
        .body(context.node.clone())
}

pub async fn webfinger(req: HttpRequest, context: web::Data<Context>) -> HttpResponse {
    if req.query_string().len() > 0 {
        let parts: Vec<&str> = req.query_string().split('@').collect();
        if parts.len() > 1 && parts[parts.len() - 1] == context.config.instance {
            if let Some(v) = context
                .replies
                .get("GET_%2Fwebfinger%3Dsource%3Fsource%3Dacct%3Apeertube.json")
            {
                return HttpResponse::Ok()
                    .content_type("application/activity+json")
                    .body(v.to_string());
            }
        }
    }

    HttpResponse::Ok()
        .content_type("application/activity+json")
        .body("")
}

pub async fn nodeinfo_meta(_query: web::Query<HashMap<String, String>>) -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/activity+json")
        .body("")
}

pub async fn index(context: web::Data<Context>) -> HttpResponse {
    if let Some(v) = context.replies.get("GET_index.html") {
        HttpResponse::Ok()
            .content_type("text/html")
            .body(v.to_string())
    } else {
        HttpResponse::InternalServerError().body("Missing expected request extension data")
    }
}

pub async fn get_accounts(req: HttpRequest, context: web::Data<Context>) -> HttpResponse {
    for h in &req.head().headers {
        trace!("{:?}", h);
    }

    let response_name = match req.query_string().len() > 0 {
        true => format!(
            "{}_{}?{}.json",
            req.method(),
            req.path(),
            req.query_string()
        ),
        false => format!("{}_{}.json", req.method(), req.path()),
    };

    let response_name_encoded = urlencoding::encode(&response_name).to_string();
    trace!("{}", response_name_encoded);
    if req.method() == Method::GET {
        trace!("GET {} {}", req.path(), req.query_string())
    }
    if let Some(v) = context.replies.get(&response_name_encoded) {
        HttpResponse::Ok()
            .content_type("application/activity+json")
            .body(v.to_string())
    } else {
        HttpResponse::Ok()
            .content_type("application/activity+json")
            .body("")
    }
}

pub async fn post_accounts(req: HttpRequest, bytes: actix_web::web::Bytes) -> HttpResponse {
    for h in &req.head().headers {
        trace!("{:?}", h);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(payload) => {
            trace!("POST {} {}", req.path(), payload);
            HttpResponse::Ok()
                .content_type("application/activity+json")
                .body("")
        }
        Err(_) => {
            error!("Failed to parse body");
            HttpResponse::BadRequest().body("Failed to parse body")
        }
    }
}

pub async fn inbox(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
    dispatcher: web::Data<relay::Dispatcher>,
) -> HttpResponse {
    for h in &req.head().headers {
        trace!("{:?}", h);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(payload) => {
            trace!("POST {} {}", req.path(), payload);

            let j_res: Result<serde_json::Value, _> = serde_json::from_str(&payload);
            if let Ok(j) = j_res {
                match (
                    j.get("id").and_then(|v| v.as_str()),
                    j.get("type").and_then(|v| v.as_str()),
                    j.get("actor").and_then(|v| v.as_str()),
                    j.get("object").and_then(|v| v.as_str()),
                    j.get("signature"),
                ) {
                    (Some(_id), Some(t), Some(a), Some(o), Some(_s)) => match t {
                        "Like" => {
                            trace!("like: {} -> {}", a, o);
                            match util::prepare_message(req, "/inbox", payload) {
                                Ok(msg) => {
                                    match dispatcher.send(msg).await {
                                        Ok(_) => {
                                            return HttpResponse::Ok()
                                                .content_type("application/activity+json")
                                                .body("");
                                        }
                                        Err(_e) => {
                                            return HttpResponse::InternalServerError().body("");
                                        }
                                    };
                                }
                                Err(e) => {
                                    error!("Failed to prepare message: {}", e);
                                }
                            }
                        }
                        "Follow" => {
                            trace!("follow: {} -> {}", a, o);
                            match util::prepare_message(req, "/inbox", payload) {
                                Ok(msg) => {
                                    match dispatcher.send(msg).await {
                                        Ok(_) => {
                                            return HttpResponse::Ok()
                                                .content_type("application/activity+json")
                                                .body("");
                                        }
                                        Err(_e) => {
                                            return HttpResponse::InternalServerError().body("");
                                        }
                                    };
                                }
                                Err(e) => {
                                    error!("Failed to prepare message: {}", e);
                                }
                            }
                        }
                        _ => {
                            error!("Unsopported type: {}", t);
                        }
                    },
                    _ => {
                        error!("Failed to parse header: {}", payload)
                    }
                }
            }
        }
        Err(_) => {
            error!("Failed to parse body");
            return HttpResponse::BadRequest().body("");
        }
    }
    HttpResponse::BadRequest().body("")
}

pub async fn post_accounts_endpoint(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
    dispatcher: web::Data<relay::Dispatcher>,
) -> HttpResponse {
    trace!("post_accounts_endpoint");

    for h in &req.head().headers {
        trace!("{:?}", h);
    }
    match (
        req.match_info().get("account_id"),
        req.match_info().get("end_point"),
    ) {
        (Some(account_id), Some(end_point)) => {
            if account_id == "peertube" && end_point == "inbox" {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(payload) => {
                        trace!("POST {} {}", req.path(), payload);

                        let j_res: Result<serde_json::Value, _> = serde_json::from_str(&payload);
                        if let Ok(j) = j_res {
                            match (
                                j.get("id"),
                                j.get("type"),
                                j.get("actor"),
                                j.get("object"),
                                j.get("@context"),
                                j.get("signature"),
                            ) {
                                (Some(_id), Some(t), Some(a), Some(o), Some(_c), Some(_s)) => {
                                    if t == "Like" {
                                        trace!("like: {} -> {}", a, o);
                                    } else if t == "Follow" {
                                        trace!("follow: {} -> {}", a, o);
                                        match a.as_str() {
                                            Some(actor) => {
                                                trace!("actor: {}", actor);
                                                let path = format!(
                                                    "/accounts/{}/{}",
                                                    account_id, end_point
                                                )
                                                .to_string();
                                                match util::prepare_message(req, &path, payload) {
                                                    Ok(msg) => {
                                                        match dispatcher.send(msg).await {
                                                            Ok(_) => {}
                                                            Err(_e) => {
                                                                return HttpResponse::InternalServerError().body("");
                                                            }
                                                        };
                                                    }
                                                    Err(e) => {
                                                        error!("Failed to prepare message: {}", e);
                                                    }
                                                }
                                            }
                                            None => {}
                                        }
                                    }
                                }
                                _ => {
                                    warn!("Message ignored: {}", payload);
                                }
                            }

                            if j["type"] == "Accept" || j["type"] == "Follow" {
                                trace!("follow: {}", j["id"]);

                                return HttpResponse::Ok()
                                    .content_type("application/activity+json")
                                    .body("");
                            }
                        }
                    }
                    Err(_) => {
                        error!("Failed to parse body");
                        return HttpResponse::BadRequest().body("");
                    }
                }
            }
        }
        _ => {
            error!("Failed to parse accounts endpoint.")
        }
    }
    HttpResponse::BadRequest().body("")
}

pub async fn post_api_follow(
    req: HttpRequest,
    query: web::Query<HashMap<String, String>>,
    context: web::Data<Context>,
) -> HttpResponse {
    for h in &req.head().headers {
        trace!("{:?}", h);
    }

    match (req.match_info().get("platform"), query.get("instance")) {
        (Some(platform), Some(instance)) => {
            if platform == "peertube" {
                match peertube::follow(instance, platform, &context).await {
                    Ok(_) => {}
                    Err(e) => {
                        error!("Error: {}", e.to_string());
                        return HttpResponse::InternalServerError().body("");
                    }
                }

                return HttpResponse::Ok()
                    .content_type("application/activity+json")
                    .body("");
            }
        }
        _ => {
            error!("Failed to find platform or instance.")
        }
    }

    HttpResponse::BadRequest().body("")
}

pub async fn post_api_test_relay(
    bytes: actix_web::web::Bytes,
    query: web::Query<HashMap<String, String>>,
    context: web::Data<Context>,
) -> HttpResponse {
    match (String::from_utf8(bytes.to_vec()), query.get("instance")) {
        (Ok(payload), Some(instance)) => {
            let j_res: Result<serde_json::Value, _> = serde_json::from_str(&payload);
            if let Ok(mut j) = j_res {
                let key_id = format!("https://{}/accounts/peertube", context.config.instance);
                match util::sign_and_send(&instance, "/inbox", &context, &mut j, &key_id, false)
                    .await
                {
                    Ok(_v) => HttpResponse::Ok().body("ok"),
                    Err(e) => {
                        error!("Error: {}", e);
                        HttpResponse::BadRequest().body("")
                    }
                }
            } else {
                HttpResponse::BadRequest().body("")
            }
        }
        _ => {
            error!("Failed to parse request");
            HttpResponse::BadRequest().body("")
        }
    }
}

pub async fn post_root(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
    dispatcher: web::Data<relay::Dispatcher>,
    context: web::Data<Context>,
) -> HttpResponse {
    for h in &req.head().headers {
        trace!("{:?}", h);
    }

    match get_platform(&req) {
        Platform::Mastodon => match String::from_utf8(bytes.to_vec()) {
            Ok(payload) => {
                trace!("POST {} {}", req.path(), payload);

                let j_res: Result<serde_json::Value, _> = serde_json::from_str(&payload);
                match j_res {
                    Ok(j) => match j.get("type").and_then(|t| t.as_str()) {
                        Some("Announce") => {
                            match mastodon::handle_boost(req, &j, payload, dispatcher).await {
                                Ok(()) => {
                                    return HttpResponse::Accepted()
                                        .content_type("application/activity+json")
                                        .body("");
                                }
                                Err(e) => {
                                    error!("Error: {}", e.to_string());
                                    return HttpResponse::BadRequest().body("");
                                }
                            }
                        }
                        Some("Follow") => {
                            match mastodon::handle_follow(&j, dispatcher, context).await {
                                Ok(()) => {
                                    return HttpResponse::Accepted()
                                        .content_type("application/activity+json")
                                        .body("");
                                }
                                Err(e) => {
                                    error!("Error: {}", e.to_string());
                                    return HttpResponse::BadRequest().body("");
                                }
                            }
                        }
                        _ => {
                            return HttpResponse::Created()
                                .content_type("application/activity+json")
                                .body("");
                        }
                    },
                    Err(_e) => {
                        return HttpResponse::BadRequest().body("Failed to parse request");
                    }
                }
            }
            Err(_) => {
                error!("Failed to parse body");
                return HttpResponse::BadRequest().body("");
            }
        },
        _ => HttpResponse::BadRequest().body(""),
    }
}

pub fn get_platform(req: &HttpRequest) -> Platform {
    match req.head().headers.get("user-agent") {
        Some(header_value) => match header_value.to_str() {
            Ok(h) => {
                if h.find("PeerTube").is_some() {
                    Platform::PeerTube
                } else if h.find("Mastodon").is_some() {
                    Platform::Mastodon
                } else {
                    Platform::Unknown
                }
            }
            Err(_) => Platform::Unknown,
        },
        None => Platform::Unknown,
    }
}
