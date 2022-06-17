use rocksdb::{DBWithThreadMode, IteratorMode, MultiThreaded};
use uuid::Uuid;
#[macro_use]
extern crate serde;
use futures::StreamExt;
use lru::LruCache;
use sha2::Digest;
use smartlike_embed_lib::client::{Client, Like};
use std::{fs::File, io::prelude::*, thread, time::Duration};
use telegram_bot::*;
#[macro_use]
extern crate log;

#[derive(Deserialize, Serialize, Clone, Default, Debug)]
pub struct Configuration {
    telegram_bot_token: String,
    num_relay_threads: usize,
    network_address: String,
    smartlike_account: String,
    smartlike_key: String,
    log_target: String,
    media_group_id_cache_size: usize,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let matches = clap::App::new("smartlike-telegram-bot")
        .version(option_env!("CARGO_PKG_VERSION").unwrap_or(""))
        .about("smartlike-telegram-bot")
        .arg(
            clap::Arg::with_name("config")
                .short("c")
                .long("config")
                .help("Configuration file")
                .takes_value(true)
                .default_value(""),
        )
        .get_matches();

    let config = matches.value_of("config").unwrap();
    let mut f = File::open(config).unwrap();
    let mut contents = String::new();
    f.read_to_string(&mut contents).unwrap();
    let config = toml::from_str::<Configuration>(&contents).unwrap();

    let db =
        std::sync::Arc::new(DBWithThreadMode::<MultiThreaded>::open_default("./queue").unwrap());

    let client = Client::new(
        config.smartlike_account.clone(),
        config.smartlike_key,
        config.network_address,
    );

    let (tx, rx) = async_channel::unbounded::<(String, Like)>();

    // Load the queue from previous run.
    let iter = db.iterator(IteratorMode::Start);
    for (key, value) in iter {
        info!("Found pending request {:?} {:?}", key, value);
        if let (Ok(k), Ok(v)) = (
            String::from_utf8(key.to_vec()),
            String::from_utf8(value.to_vec()),
        ) {
            if let Ok(like) = serde_json::from_str(&v) {
                match tx.send((k, like)).await {
                    Ok(_) => {
                        continue;
                    }
                    Err(e) => {
                        error!("TX Error: {}", e);
                    }
                }
            }
            error!("Failed to enqueue pending receipt. Rejecting.");
            match db.delete(key) {
                Ok(_) => {}
                Err(e) => {
                    error!("Failed to delete db record: {}", e);
                }
            }
        }
    }

    tokio::spawn({
        let client = client.clone();
        let db = db.clone();
        let tx = tx.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(msg) => {
                        match client.forward_like(&msg.1).await {
                            Ok(_) => db.delete(msg.0).unwrap_or_else(|e| {
                                panic!("Failed to delete db record: {}", e)
                            }),
                            Err(e) => {
                                // Communications issues? - Wait and retry.
                                error!("Failed to forward like: {}", e);
                                thread::sleep(Duration::from_secs(5));
                                tx.send(msg).await.unwrap_or_else(|e| panic!("TX Error: {}", e));
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
        }
    });

    let mut media_group_ids: LruCache<String, String> =
        LruCache::new(config.media_group_id_cache_size);
    let api = Api::new(config.telegram_bot_token);
    let mut stream = api.stream();

    while let Some(update) = stream.next().await {
        // If the received update contains a new message...
        let update = update?;
        if let UpdateKind::Message(msg) = update.kind {
            trace!("message: {:?}", &msg);

            if msg.from.is_bot {
                warn!("Bot ignored");
                continue;
            }

            match &msg.kind {
                telegram_bot::types::MessageKind::Text { data, .. } => {
                    trace!("Text: {}", data);
                    match data.find("/start ") {
                        Some(v) => {
                            if v == 0 {
                                let pars: Vec<&str> = data.split(' ').collect();
                                if pars.len() == 2 {
                                    let args: Vec<&str> = pars[1].split('_').collect();
                                    if args.len() == 2 {
                                        match (Uuid::parse_str(args[0]), args[1].parse::<u32>()) {
                                            (Ok(account), Ok(donation)) => {
                                                let mut username = msg.from.first_name.clone();
                                                if let Some(v) = msg.from.last_name.clone() {
                                                    username.push_str(" ");
                                                    username.push_str(&v);
                                                }
                                                if let Some(v) = msg.from.username.clone() {
                                                    username.push_str(" (@");
                                                    username.push_str(&v);
                                                    username.push_str(")");
                                                }
                                                let float_donation = donation as f32 / 100.0;
                                                let to_sign = format!(
                                                    "telegram{}{}{}",
                                                    msg.from.id, account, float_donation
                                                );
                                                let sig = client.sign(&to_sign);
                                                let url = format!("https://smartlike.org/confirm?platform=telegram&id={}&name={}&account={}&amount={}&proxy={}&signature={}", msg.from.id, username, account, float_donation, config.smartlike_account, sig);

                                                let mut keyboard =
                                                    telegram_bot::types::InlineKeyboardMarkup::new(
                                                    );
                                                let mut v: Vec<
                                                    telegram_bot::types::InlineKeyboardButton,
                                                > = Vec::new();
                                                v.push(
                                                    telegram_bot::types::InlineKeyboardButton::url(
                                                        "confirm", url,
                                                    ),
                                                );
                                                keyboard.add_row(v);

                                                api.spawn(msg.to_source_chat().text(
                                                    "Smartlike bot is a free micro-donation processor. Forward your favorite posts to the bot to support authors and help other users discover great content.\n<a href=\"https://smartlike.org/docs\">read more</a> | <a href='https://smartlike.org/channel/t.me'>charts</a>\n\nPlease follow the link to confirm connection to your Smartlike account:"
                                                ).parse_mode(telegram_bot::types::ParseMode::Html).reply_markup(keyboard));
                                            }
                                            (_, _) => {
                                                api.spawn(
                                                    msg.to_source_chat()
                                                        .text("Wrong parameter(s)."),
                                                );
                                            }
                                        }
                                    }
                                }
                                continue;
                            }
                        }
                        _ => {}
                    }

                    if data == "/settings" {
                        let mut keyboard = telegram_bot::types::InlineKeyboardMarkup::new();
                        let mut v: Vec<telegram_bot::types::InlineKeyboardButton> = Vec::new();
                        v.push(telegram_bot::types::InlineKeyboardButton::url(
                            "settings",
                            "https://smartlike.org/docs/how-to-connect-telegram",
                        ));
                        keyboard.add_row(v);
                        api.spawn(
                            msg.to_source_chat()
                                .text("Please follow the link for settings.")
                                .reply_markup(keyboard),
                        );
                        continue;
                    } else if data == "/help" {
                        api.spawn(msg.to_source_chat().text(
                            "Smartlike bot is a free micro-donation processor. Forward your favorite posts to the bot to support authors and help other users discover great content.\n<a href=\"https://smartlike.org/docs\">read more</a> | <a href='https://smartlike.org/channel/t.me'>charts</a> | <a href='https://smartlike.org/docs/how-to-connect-telegram'>settings</a>"
                        ).parse_mode(telegram_bot::types::ParseMode::Html));
                        continue;
                    } else if data == "/start" {
                        let url = "https://smartlike.org/docs/how-to-connect-telegram";

                        let mut keyboard = telegram_bot::types::InlineKeyboardMarkup::new();
                        let mut v: Vec<telegram_bot::types::InlineKeyboardButton> = Vec::new();
                        v.push(telegram_bot::types::InlineKeyboardButton::url(
                            "connect", url,
                        ));
                        keyboard.add_row(v);

                        api.spawn(
                            msg.to_source_chat().text(
                                "Smartlike bot is a free micro-donation processor. Forward your favorite posts to the bot to support authors and help other users discover great content.\n<a href=\"https://smartlike.org/docs\">read more</a> | <a href='https://smartlike.org/channel/t.me'>charts</a>\n\nPlease follow the link to connect:",
                            )
                            .parse_mode(telegram_bot::types::ParseMode::Html).reply_markup(keyboard));
                        continue;
                    }
                }
                telegram_bot::types::MessageKind::Photo { media_group_id, .. } => {
                    if let Some(id) = media_group_id {
                        if media_group_ids.contains(id) {
                            debug!("Skipping media group {}", id);
                            continue;
                        } else {
                            media_group_ids.put(id.to_string(), id.to_string());
                        }
                    }
                }
                telegram_bot::types::MessageKind::Video { media_group_id, .. } => {
                    if let Some(id) = media_group_id {
                        if media_group_ids.contains(id) {
                            debug!("Skipping media group {}", id);
                            continue;
                        } else {
                            media_group_ids.put(id.to_string(), id.to_string());
                        }
                    }
                }

                _ => {
                    trace!("No message id.");
                }
            }

            match msg.forward {
                Some(forward) => {
                    let mut channel_id = None;
                    let mut msg_id = 0;
                    match forward.from {
                        telegram_bot::types::ForwardFrom::Channel {
                            channel,
                            message_id,
                            ..
                        } => match channel.username {
                            Some(username) => {
                                channel_id = Some(username);
                                msg_id = message_id;
                            }
                            _ => {
                                warn!("No message username.");
                            }
                        },
                        _ => {
                            warn!("No message id.");
                        }
                    }

                    match channel_id {
                        Some(id) => {
                            let like = Like {
                                platform: "telegram".to_string(),
                                id: msg.from.id.to_string(),
                                target: format!("https://t.me/{}/{}", id, msg_id).to_string(),
                                amount: 0.0,
                                currency: "".to_string(),
                            };

                            match serde_json::to_string(&like) {
                                Ok(message) => {
                                    let mut hasher = sha2::Sha256::new();
                                    sha2::Digest::input(&mut hasher, message.as_bytes());
                                    let key = hex::encode(hasher.result().as_slice().to_vec());

                                    match db.put(key.clone(), message.clone()) {
                                        Ok(_) => match tx.send((key, like)).await {
                                            Ok(_) => {}
                                            Err(e) => {
                                                error!("TX Error: {}", e);
                                            }
                                        },
                                        Err(e) => {
                                            error!("DB error: {}", e);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {
                            error!("No user id.");
                        }
                    }
                }
                _ => {
                    api.spawn(msg.to_source_chat().text("Unknown command. Please use /help, /settings or forward posts to this bot."));
                }
            }
        }
    }

    Ok(())
}
