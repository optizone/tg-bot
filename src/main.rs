use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use group_handlers::Chat;
use mongodb::{options::ClientOptions, Client};
use private_handlers::Private;
use std::sync::RwLock;
use teloxide::prelude::*;
use tokio::sync::Mutex;

use derive_more::From;
use teloxide::macros::Transition;

mod db_utils;
mod error;
mod extractors;
mod group_handlers;
mod private_handlers;

#[derive(Clone)]
pub struct Echo;

#[teloxide(subtransition)]
async fn echo(_: Echo, cx: TransitionIn<AutoSend<Bot>>, _: String) -> TransitionOut<Dialogue> {
    while let Err(teloxide::RequestError::RetryAfter(secs)) =
        cx.answer(cx.update.text().unwrap_or("Echo")).await
    {
        tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
    }
    next(Echo)
}

#[derive(Transition, From, Clone)]
pub enum Dialogue {
    Echo(Echo),
    Private(Private),
    Chat(Chat),
}

impl Default for Dialogue {
    fn default() -> Self {
        Dialogue::Echo(Echo)
    }
}

#[tokio::main]
async fn main() {
    run().await
}

lazy_static::lazy_static! {
    pub static ref MESSAGES: Mutex<HashMap<i64, Vec<Message>>> = Mutex::new(HashMap::new());
    pub static ref LAST_REGION: Mutex<String> = Mutex::new(String::new());
    pub static ref ALL_TAGS: RwLock<HashSet<&'static str>> = RwLock::new(HashSet::new());
    pub static ref ALL_REGIONS: RwLock<HashSet<&'static str>> = RwLock::new(HashSet::new());
    pub static ref ALLIAS_REGIONS: RwLock<HashMap<&'static str, &'static str>> = RwLock::new(HashMap::new());
    pub static ref ALL_CHATS: tokio::sync::RwLock<HashSet<i64>> = tokio::sync::RwLock::new(HashSet::new());
}

async fn run() {
    dotenv::dotenv().unwrap();

    let mut options = ClientOptions::parse(std::env::var("MONGODB_URI").unwrap())
        .await
        .unwrap();

    // options.credential = Some(
    //     Credential::builder()
    //         // .username(std::env::var("MONGODB_CREDENTIAL_USER").ok())
    //         .source(std::env::var("MONGODB_CREDENTIAL_SOURCE").ok())
    //         // .password(std::env::var("MONGODB_CREDENTIAL_PASSWORD").ok())
    //         // .mechanism(std::env::var("MONGODB_CREDENTIAL_MECHANISM").ok())
    //         // .mechanism_properties(std::env::var("MONGODB_CREDENTIAL_PROPERTIES").ok())
    //         .build(),
    // );

    options.min_pool_size = std::env::var("MONGODB_POLL_MIN_CONNECTIONS")
        .ok()
        .map(|s| s.parse().unwrap());
    options.max_pool_size = std::env::var("MONGODB_POLL_MAX_CONNECTIONS")
        .ok()
        .map(|s| s.parse().unwrap());

    let _mongo_client = Arc::new(Client::with_options(options).expect("failed to connect"));
    let mongo_client = unsafe { &*(&_mongo_client as *const Arc<_>) as &'static Arc<_> };
    std::mem::forget(_mongo_client);

    pretty_env_logger::formatted_timed_builder()
        .write_style(pretty_env_logger::env_logger::WriteStyle::Auto)
        .filter(
            Some(&env!("CARGO_PKG_NAME").replace("-", "_")),
            log::LevelFilter::Trace,
        )
        .filter(Some("teloxide"), log::LevelFilter::Info)
        .format_timestamp_secs()
        .init();
    log::info!("Starting bot");

    db_utils::validate_db(&mongo_client).await.unwrap();

    let (all_regions, alias_regions) = {
        let mut regions = db_utils::get_regions(&mongo_client)
            .await
            .expect("Can't access regions. Bad response from server");
        for r in &mut regions {
            r.aliases.iter_mut().for_each(|a| *a = a.to_lowercase());
        }

        let res = unsafe {
            (
                regions
                    .iter()
                    .map(|r| &*(r.region.as_str() as *const str) as &'static str)
                    .collect::<HashSet<_>>(),
                regions
                    .iter()
                    .map(|r| {
                        let mut a = r
                            .aliases
                            .iter()
                            .map(|a| {
                                (
                                    &*(a.as_str() as *const str) as &'static str,
                                    &*(r.region.as_str() as *const str) as &'static str,
                                )
                            })
                            .collect::<Vec<_>>();
                        a.push((
                            &*(r.region.as_str() as *const str) as &'static str,
                            &*(r.region.as_str() as *const str) as &'static str,
                        ));
                        a
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .flatten()
                    .collect::<HashMap<_, _>>(),
            )
        };
        let (a, b) = &res;
        let res2 = unsafe {
            (
                &*(a as *const HashSet<&str>),
                &*(b as *const HashMap<&str, &str>),
            )
        };
        std::mem::forget(regions);
        std::mem::forget(res);

        res2
    };

    {
        let mut al = ALLIAS_REGIONS.write().unwrap();
        let mut all = ALL_REGIONS.write().unwrap();
        let mut tags = ALL_TAGS.write().unwrap();
        let mut chats = ALL_CHATS.write().await;
        db_utils::get_chats(&mongo_client)
            .await
            .expect("Can't access chats. Bad response from server.")
            .into_iter()
            .for_each(|c| {
                chats.insert(c);
            });
        db_utils::get_tags(&mongo_client)
            .await
            .expect("Can't access tags. Bad response from server.")
            .into_iter()
            .for_each(|t| unsafe {
                tags.insert(&*(t.as_str() as *const str) as &'static str);
                std::mem::forget(t);
            });
        alias_regions.into_iter().for_each(|(k, v)| {
            al.insert(k, v);
        });
        all_regions.into_iter().for_each(|v| {
            all.insert(v);
        });
    }

    let bot = Bot::from_env().auto_send();

    teloxide::dialogues_repl(bot, move |cx, dialogue: Dialogue| async move {
        let chat = cx.requester.get_chat(cx.chat_id()).await.unwrap();
        let text = cx.update.text();
        let private = chat.is_private();
        let chat_in_table = ALL_CHATS.read().await.contains(&cx.chat_id());

        if cx.update.super_group_chat_created().is_some() {
            log::trace!(
                "Super chat created: \"{}\". Migrate from: {}. Migrate to: {}",
                chat.title().unwrap_or_default(),
                cx.update.migrate_from_chat_id().unwrap_or_default(),
                cx.update.migrate_to_chat_id().unwrap_or_default()
            );
            match (
                cx.update.migrate_from_chat_id(),
                cx.update.migrate_to_chat_id(),
            ) {
                (Some(from_id), Some(to_id)) => {
                    if let Err(e) = db_utils::migrate_chat(mongo_client, from_id, to_id).await {
                        log::error!(
                            "Can't migrate chat from {} to {}. Error: {}",
                            from_id,
                            to_id,
                            e.to_string()
                        )
                    }
                }
                (None, Some(_)) => log::error!("Cant migrate to superchat: from_id isn't set"),
                (Some(_), None) => log::error!("Cant migrate to superchat: to_id isn't set"),
                (None, None) => {
                    log::error!("Cant migrate to superchat: from_id and to_id isn't set")
                }
            }
        }

        if private {
            log::trace!(
                "Username: \"{}\". User id: {}. Text: \"{}\"",
                cx.update
                    .from()
                    .as_ref()
                    .unwrap()
                    .username
                    .as_ref()
                    .unwrap_or(&String::new()),
                cx.update.from().unwrap().id,
                text.unwrap_or_default()
            );
        } else {
            log::trace!(
                "Chat title: \"{}\". Chat id: {}. Text: \"{}\"",
                chat.title().unwrap_or_default(),
                chat.id,
                text.unwrap_or_default()
            );
        }
        match (private, &dialogue) {
            (true, Dialogue::Echo(_)) => Dialogue::from(Private::new(
                Arc::clone(mongo_client),
                cx.update.from().unwrap().id,
            ))
            .react(cx, String::new())
            .await
            .unwrap(),
            (false, Dialogue::Echo(_)) if !chat_in_table => {
                dialogue.react(cx, String::new()).await.unwrap()
            }
            (false, Dialogue::Echo(_)) if chat_in_table => {
                Dialogue::from(Chat::new(Arc::clone(mongo_client)))
                    .react(cx, String::new())
                    .await
                    .unwrap()
            }
            (true, Dialogue::Private(_)) => dialogue.react(cx, String::new()).await.unwrap(),
            (false, Dialogue::Chat(_)) if chat_in_table => {
                dialogue.react(cx, String::new()).await.unwrap()
            }
            (false, Dialogue::Chat(_)) if !chat_in_table => {
                Dialogue::from(Echo).react(cx, String::new()).await.unwrap()
            }
            _ => unreachable!(),
        }
    })
    .await;
}
