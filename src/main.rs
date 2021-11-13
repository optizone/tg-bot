use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use group_handlers::Chat;
use mongodb::{options::ClientOptions, Client};
use private_handlers::Private;
use std::sync::RwLock;
use teloxide::{prelude::*, types::MessageKind, RequestError};
use tokio::sync::Mutex;

use derive_more::From;
use teloxide::macros::Transition;

mod common;
mod db_utils;
mod error;
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
    dotenv::dotenv().expect("Can't access .env file");

    let mut options = ClientOptions::parse(
        std::env::var("MONGODB_URI").expect("MONGODB_URI enviroment variable must be set!"),
    )
    .await
    .expect("Can't parse MONGODB_URI as ClientOptions");

    // options.credential = Some(
    //     Credential::builder()
    //         // .username(std::env::var("MONGODB_CREDENTIAL_USER").ok())
    //         .source(std::env::var("MONGODB_CREDENTIAL_SOURCE").ok())
    //         // .password(std::env::var("MONGODB_CREDENTIAL_PASSWORD").ok())
    //         // .mechanism(std::env::var("MONGODB_CREDENTIAL_MECHANISM").ok())
    //         // .mechanism_properties(std::env::var("MONGODB_CREDENTIAL_PROPERTIES").ok())
    //         .build(),
    // );

    options.min_pool_size = std::env::var("MONGODB_POLL_MIN_CONNECTIONS").ok().map(|s| {
        s.parse()
            .expect("Can't parse MONGODB_POLL_MIN_CONNECTIONS as u32")
    });
    options.max_pool_size = std::env::var("MONGODB_POLL_MAX_CONNECTIONS").ok().map(|s| {
        s.parse()
            .expect("Can't parse MONGODB_POLL_MAX_CONNECTIONS as u32")
    });

    let mongo_client = Box::leak(Box::new(Arc::new(
        Client::with_options(options).expect("failed to connect to mongodb"),
    ))) as &'static Arc<_>;

    log4rs::init_file("log4rs.yml", Default::default())
        .expect("Can't init logger from file log4rs.yml");
    log::info!("Starting bot");

    db_utils::validate_db(&mongo_client)
        .await
        .expect("Can't validate database");

    let (all_regions, alias_regions) = {
        let regions = db_utils::get_regions(&mongo_client)
            .await
            .expect("Can't access regions. Bad response from server");
        let regions = regions
            .into_iter()
            .map(|mut r| {
                r.aliases.iter_mut().for_each(|a| *a = a.to_lowercase());
                r.aliases.push(r.region.clone());

                let aliases = r
                    .aliases
                    .into_iter()
                    .map(|a| Box::leak(Box::new(a)))
                    .collect::<Vec<_>>();

                let region = Box::leak(Box::new(r.region));
                (region, aliases)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(r, a)| {
                (
                    r.as_str(),
                    a.into_iter().map(|a| a.as_str()).collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let re = regions.iter().map(|&(r, _)| r).collect::<HashSet<_>>();
        let al = regions
            .iter()
            .map(|(r, a)| a.iter().map(|&a| (a, *r)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
            .into_iter()
            .flatten()
            .collect::<HashMap<_, _>>();

        (Box::leak(Box::new(re)), Box::leak(Box::new(al)))
    };

    {
        let mut al = ALLIAS_REGIONS
            .write()
            .map_err(|e| log::error!("Can't lock ALLIAS_REGIONS. Error: {}", e.to_string()))
            .unwrap();
        let mut all = ALL_REGIONS
            .write()
            .map_err(|e| log::error!("Can't lock ALL_REGIONS. Error: {}", e.to_string()))
            .unwrap();
        let mut tags = ALL_TAGS
            .write()
            .map_err(|e| log::error!("Can't lock ALL_TAGS. Error: {}", e.to_string()))
            .unwrap();
        let mut chats = ALL_CHATS.write().await;
        db_utils::get_chats(mongo_client)
            .await
            .expect("Can't access chats. Bad response from server.")
            .into_iter()
            .for_each(|c| {
                chats.insert(c);
            });
        db_utils::get_tags(mongo_client)
            .await
            .expect("Can't access tags. Bad response from server.")
            .into_iter()
            .for_each(|t| {
                tags.insert(Box::leak(Box::new(t)).as_str() as &'static str);
            });
        alias_regions.iter().for_each(|(&k, &v)| {
            al.insert(k, v);
        });
        all_regions.iter().for_each(|&v| {
            all.insert(v);
        });
    }

    let bot = Bot::from_env().auto_send();

    teloxide::dialogues_repl(bot, move |cx, dialogue: Dialogue| async move {
        let chat = cx
            .requester
            .get_chat(cx.chat_id())
            .await
            .map_err(|e| log::error!("Can't get chat from context. Error: {}", e.to_string()))
            .unwrap();
        let text = cx.update.text();
        let private = chat.is_private();
        let chat_in_table = ALL_CHATS.read().await.contains(&cx.chat_id());

        match cx.update.kind {
            MessageKind::Common(_) => {}
            MessageKind::Migrate(m) => {
                log::trace!(
                    "Super chat created: \"{}\". Migrate from: {}. Migrate to: {}",
                    chat.title().unwrap_or_default(),
                    m.migrate_from_chat_id,
                    m.migrate_to_chat_id
                );
                if let Err(e) = db_utils::migrate_chat(
                    mongo_client,
                    m.migrate_from_chat_id,
                    m.migrate_to_chat_id,
                )
                .await
                {
                    log::error!(
                        "Can't migrate chat from {} to {}. Error: {}",
                        m.migrate_from_chat_id,
                        m.migrate_to_chat_id,
                        e.to_string()
                    )
                }
                return next::<_, _, RequestError>(dialogue)
                    .map_err(|e| log::error!("Error while skipping message: {}", e.to_string()))
                    .unwrap();
            }
            m => {
                log::debug!("Unhandlable message: {:?}", m);
                return next::<_, _, RequestError>(dialogue)
                    .map_err(|e| log::error!("Error while skipping message: {}", e.to_string()))
                    .unwrap();
            }
        }

        if private {
            let (username, id) = match cx.update.from() {
                Some(u) => (u.username.as_ref().map(|s| s.as_str()).unwrap_or(""), u.id),
                None => {
                    log::error!("Can't access `from` from update");
                    ("", 0)
                }
            };
            log::info!(
                "Username: \"{}\". User id: {}. Text: \"{}\"",
                username,
                id,
                text.unwrap_or_default()
            );
        } else {
            log::info!(
                "Chat title: \"{}\". Chat id: {}. Text: \"{}\"",
                chat.title().unwrap_or_default(),
                chat.id,
                text.unwrap_or_default()
            );
        }
        match (private, &dialogue) {
            (true, Dialogue::Echo(_)) => Dialogue::from(Private::new(
                Arc::clone(mongo_client),
                match cx.update.from() {
                    Some(u) => u.id,
                    None => {
                        log::error!("Can't access `from` from update, panicing...");
                        panic!()
                    }
                },
            ))
            .react(cx, String::new())
            .await
            .map_err(|e| {
                log::error!(
                    "Error while reacting update [{file}/{line}]: {err}",
                    err = e.to_string(),
                    file = file!(),
                    line = line!(),
                )
            })
            .unwrap(),
            (false, Dialogue::Echo(_)) if !chat_in_table => dialogue
                .react(cx, String::new())
                .await
                .map_err(|e| {
                    log::error!(
                        "Error while reacting update [{file}/{line}]: {err}",
                        err = e.to_string(),
                        file = file!(),
                        line = line!(),
                    )
                })
                .unwrap(),
            (false, Dialogue::Echo(_)) if chat_in_table => {
                Dialogue::from(Chat::new(Arc::clone(mongo_client)))
                    .react(cx, String::new())
                    .await
                    .map_err(|e| {
                        log::error!(
                            "Error while reacting update [{file}/{line}]: {err}",
                            err = e.to_string(),
                            file = file!(),
                            line = line!(),
                        )
                    })
                    .unwrap()
            }
            (true, Dialogue::Private(_)) => dialogue
                .react(cx, String::new())
                .await
                .map_err(|e| {
                    log::error!(
                        "Error while reacting update [{file}/{line}]: {err}",
                        err = e.to_string(),
                        file = file!(),
                        line = line!(),
                    )
                })
                .unwrap(),
            (false, Dialogue::Chat(_)) if chat_in_table => dialogue
                .react(cx, String::new())
                .await
                .map_err(|e| {
                    log::error!(
                        "Error while reacting update [{file}/{line}]: {err}",
                        err = e.to_string(),
                        file = file!(),
                        line = line!(),
                    )
                })
                .unwrap(),
            (false, Dialogue::Chat(_)) if !chat_in_table => Dialogue::from(Echo)
                .react(cx, String::new())
                .await
                .map_err(|e| {
                    log::error!(
                        "Error while reacting update [{file}/{line}]: {err}",
                        err = e.to_string(),
                        file = file!(),
                        line = line!(),
                    )
                })
                .unwrap(),
            _ => unreachable!(),
        }
    })
    .await;
}
