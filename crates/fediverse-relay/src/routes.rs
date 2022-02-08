use crate::{context::Context, peertube, relay, util};
use actix_web::{http::Method, web, HttpRequest, HttpResponse};
use std::collections::HashMap;

pub async fn nodeinfo(context: web::Data<Context>) -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/activity+json")
        .body(context.node.clone()))
}

pub async fn webfinger(
    req: HttpRequest,
    context: web::Data<Context>,
) -> actix_web::Result<HttpResponse> {
    if req.query_string().len() > 0 {
        let parts: Vec<&str> = req.query_string().split('@').collect();
        if parts.len() > 1 && parts[parts.len() - 1] == context.config.instance {
            if let Some(v) = context
                .replies
                .get("GET_%2Fwebfinger%3Dsource%3Fsource%3Dacct%3Apeertube.json")
            {
                return Ok(HttpResponse::Ok()
                    .content_type("application/activity+json")
                    .body(v));
            }
        }
    }

    Ok(HttpResponse::Ok()
        .content_type("application/activity+json")
        .body(""))
}

pub async fn nodeinfo_meta(
    _query: web::Query<HashMap<String, String>>,
) -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/activity+json")
        .body(""))
}

pub async fn actor(node_actor: web::Data<Context>) -> actix_web::Result<HttpResponse> {
    Ok(HttpResponse::Ok()
        .content_type("application/activity+json")
        .body(node_actor.actor.clone()))
}

pub async fn index(context: web::Data<Context>) -> actix_web::Result<HttpResponse> {
    if let Some(v) = context.replies.get("GET_index.html") {
        Ok(HttpResponse::Ok().content_type("text/html").body(v))
    } else {
        Err(HttpResponse::InternalServerError().into())
    }
}

pub async fn get_accounts(
    req: HttpRequest,
    context: web::Data<Context>,
) -> actix_web::Result<HttpResponse> {
    for h in &req.head().headers {
        println!("{:?}", h);
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
    println!("{}", response_name_encoded);
    if req.method() == Method::GET {
        println!("GET {} {}", req.path(), req.query_string())
    }
    if let Some(v) = context.replies.get(&response_name_encoded) {
        Ok(HttpResponse::Ok()
            .content_type("application/activity+json")
            .body(v))
    } else {
        Ok(HttpResponse::Ok()
            .content_type("application/activity+json")
            .body(""))
    }
}

pub async fn post_accounts(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
) -> actix_web::Result<HttpResponse> {
    for h in &req.head().headers {
        println!("{:?}", h);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(payload) => {
            println!("POST {}", payload);
            Ok(HttpResponse::Ok()
                .content_type("application/activity+json")
                .body(""))
        }
        Err(_) => {
            println!("Failed to parse body");
            Err(HttpResponse::BadRequest().into())
        }
    }
}

pub async fn inbox(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
    dispatcher: web::Data<relay::Dispatcher>,
) -> actix_web::Result<HttpResponse> {
    for h in &req.head().headers {
        println!("{:?}", h);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(payload) => {
            println!("POST {}", payload);

            let j: serde_json::Value = serde_json::from_str(&payload)?;
            match (
                j.get("id"),
                j.get("type"),
                j.get("actor"),
                j.get("object"),
                j.get("signature"),
            ) {
                (Some(_id), Some(t), Some(a), Some(o), Some(_s)) => {
                    if t == "Like" {
                        println!("like: {} -> {}", a, o);
                        match util::prepare_message(req, "/inbox", payload) {
                            Ok(msg) => {
                                match dispatcher.send(msg) {
                                    Ok(_) => {
                                        return Ok(HttpResponse::Ok()
                                            .content_type("application/activity+json")
                                            .body(""));
                                    }
                                    Err(_e) => {
                                        return Err(HttpResponse::InternalServerError().into());
                                    }
                                };
                            }
                            Err(e) => {
                                println!("Failed to prepare message: {}", e);
                            }
                        }
                    } else if t == "Follow" {
                        println!("follow: {} -> {}", a, o);
                        match util::prepare_message(req, "/inbox", payload) {
                            Ok(msg) => {
                                match dispatcher.send(msg) {
                                    Ok(_) => {
                                        return Ok(HttpResponse::Ok()
                                            .content_type("application/activity+json")
                                            .body(""));
                                    }
                                    Err(_e) => {
                                        return Err(HttpResponse::InternalServerError().into());
                                    }
                                };
                            }
                            Err(e) => {
                                println!("Failed to prepare message: {}", e);
                            }
                        }
                    }
                }
                (_, _, _, _, _) => {}
            }
        }
        Err(_) => {
            println!("Failed to parse body");
            return Err(HttpResponse::BadRequest().into());
        }
    }
    Err(HttpResponse::BadRequest().into())
}

pub async fn post_accounts_endpoint(
    req: HttpRequest,
    bytes: actix_web::web::Bytes,
    dispatcher: web::Data<relay::Dispatcher>,
) -> actix_web::Result<HttpResponse> {
    println!("post_accounts_endpoint");

    for h in &req.head().headers {
        println!("{:?}", h);
    }
    match (
        req.match_info().get("account_id"),
        req.match_info().get("end_point"),
    ) {
        (Some(account_id), Some(end_point)) => {
            if account_id == "peertube" && end_point == "inbox" {
                match String::from_utf8(bytes.to_vec()) {
                    Ok(payload) => {
                        println!("POST {}", payload);

                        let j: serde_json::Value = serde_json::from_str(&payload)?;
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
                                    println!("like: {} -> {}", a, o);
                                } else if t == "Follow" {
                                    println!("follow: {} -> {}", a, o);
                                    match a.as_str() {
                                        Some(actor) => {
                                            println!("actor: {}", actor);
                                            let path =
                                                format!("/accounts/{}/{}", account_id, end_point)
                                                    .to_string();
                                            match util::prepare_message(req, &path, payload) {
                                                Ok(msg) => {
                                                    match dispatcher.send(msg) {
                                                        Ok(_) => {}
                                                        Err(_e) => {
                                                            return Err(
                                                                HttpResponse::InternalServerError()
                                                                    .into(),
                                                            );
                                                        }
                                                    };
                                                }
                                                Err(e) => {
                                                    println!("Failed to prepare message: {}", e);
                                                }
                                            }
                                        }
                                        None => {}
                                    }
                                }
                            }
                            (_, _, _, _, _, _) => {
                                println!("Message ignored");
                            }
                        }

                        if j["type"] == "Accept" || j["type"] == "Follow" {
                            println!("follow: {}", j["id"]);

                            return Ok(HttpResponse::Ok()
                                .content_type("application/activity+json")
                                .body(""));
                        }
                    }
                    Err(_) => {
                        println!("Failed to parse body");
                        return Err(HttpResponse::BadRequest().into());
                    }
                }
            }
        }
        (_, _) => {}
    }
    Err(HttpResponse::BadRequest().into())
}

pub async fn post_api_follow(
    req: HttpRequest,
    query: web::Query<HashMap<String, String>>,
    context: web::Data<Context>,
) -> actix_web::Result<HttpResponse> {

    for h in &req.head().headers {
        println!("{:?}", h);
    }

    match (req.match_info().get("platform"), query.get("instance")) {
        (Some(platform), Some(instance)) => {
            if platform == "peertube" {
                match peertube::follow(instance, platform, &context).await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {}", e.to_string());
                        return Err(HttpResponse::InternalServerError().into());
                    }
                }

                return Ok(HttpResponse::Ok()
                    .content_type("application/activity+json")
                    .body(""));
            }
        }
        (_, _) => {}
    }

    Err(HttpResponse::BadRequest().into())
}

pub async fn post_api_test_relay(
    bytes: actix_web::web::Bytes,
    query: web::Query<HashMap<String, String>>,
    context: web::Data<Context>,
) -> actix_web::Result<String> {

    match (String::from_utf8(bytes.to_vec()), query.get("instance")) {
        (Ok(payload), Some(instance)) => {

            let mut j: serde_json::Value = serde_json::from_str(&payload)?;
            let key_id = format!("https://{}/accounts/peertube", context.config.instance);
        
            match util::sign_and_send(
                &instance,
                "/inbox",
                &context,
                &mut j,
                &key_id,
                false,
            )
            .await {
                Ok(_v) => Ok("ok".to_string()),
                Err(e) => {
                    println!("Error: {}", e);
                    Err(HttpResponse::BadRequest().into())
                }
            }
        }
        (_, _) => {
            println!("Failed to parse request");
            Err(HttpResponse::BadRequest().into())
        }
    }
}
