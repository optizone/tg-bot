use crate::ALL_CHATS;
use bson::oid::ObjectId;
use chrono::Duration;
use mongodb::Client;
use std::{collections::BTreeMap, str::FromStr, sync::Arc};
use teloxide::prelude::*;

lazy_static::lazy_static! {
    static ref GET_REGEX: regex::Regex
        = regex::Regex::new(r"^(?P<regions>([\p{L}-]{2,}\s*)+([\p{L}-]{2,})?)?(?P<duration>\s+\d+)?\s*(?P<tags>(\p{L}\s+)*\p{L}$)?$")
            .expect("Cant create a regex");
    static ref CMD_REGEX: regex::Regex
        = regex::Regex::new(r"^/(?P<start>start$)|(?P<help>help$)|(?P<list_users>list_users$)|(?P<add_user>add_user\s+-?\d+(\s+Admin)?$)|(?P<del_user>del_user\s+-?\d+$)|(?P<list_chats>list_chats$)|(?P<add_chat>add_chat\s+-?\d+$)|(?P<del_chat>del_chat\s+-?\d+$)|(?P<listdb>listdb(\s+\d{2}\.\d{2}\.\d{2}(\s+[+\-]\d{2}:\d{2})?)?$)|(?P<deldb>deldb\s+[0-9a-zA-Z]{24}$)|(?P<cleandb>cleandb\s+\d+$)|(?P<statdb>statdb(\s+[+\-]\d{2}:\d{2})?$)")
            .expect("Cant create a regex");
}

use crate::{
    db_utils::{self, models::UserGroup},
    error::Error,
    extractors::*,
    Dialogue,
};

#[derive(Clone)]
pub struct Private(db_utils::user::User);

impl Private {
    pub fn new(client: Arc<Client>, id: i64) -> Self {
        Self(db_utils::user::User::new(id, client))
    }
}

async fn send_str(cx: &TransitionIn<AutoSend<Bot>>, str: &str) {
    while let Err(teloxide::RequestError::RetryAfter(secs)) = cx.answer(str).await {
        tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
    }
}

async fn send_messages(
    cx: &TransitionIn<AutoSend<Bot>>,
    messages: Vec<db_utils::models::Message>,
    with_id: bool,
) {
    for message in messages {
        if with_id {
            send_str(cx, message._id.to_hex().as_str()).await;
        }
        while let Err(teloxide::RequestError::RetryAfter(secs)) = cx
            .requester
            .forward_message(cx.chat_id(), message.chat_id, message.message_id)
            .await
        {
            tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await;
        }
    }
}

#[teloxide(subtransition)]
async fn private(
    state: Private,
    cx: TransitionIn<AutoSend<Bot>>,
    _: String,
) -> TransitionOut<Dialogue> {
    let text = cx.update.text();

    if let Some(c) = CMD_REGEX.captures(text.unwrap_or_default()) {
        if let Some(_) = c.name("start").map(|m| m.as_str()) {
            match state.0.start() {
                Ok(s) => send_str(&cx, s).await,
                Err(e) => send_str(&cx, e.to_string().as_str()).await,
            }
        } else if let Some(_) = c.name("help").map(|m| m.as_str()) {
            match state.0.help().await {
                Ok(s) => send_str(&cx, s).await,
                Err(e) => send_str(&cx, e.to_string().as_str()).await,
            }
        } else if let Some(_) = c.name("list_users").map(|m| m.as_str()) {
            match state.0.list_users(vec![]).await {
                Ok(users) => {
                    for user in users {
                        let s = serde_json::to_string_pretty(&user).unwrap_or_default();
                        send_str(&cx, s.as_str()).await;
                    }
                }
                Err(e) => send_str(&cx, e.to_string().as_str()).await,
            }
        } else if let Some(add_user) = c.name("add_user").map(|m| m.as_str()) {
            let id = add_user
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or_default();
            let group = db_utils::models::UserGroup::from_str(
                add_user.split_whitespace().nth(2).unwrap_or("Registered"),
            )
            .unwrap_or(db_utils::models::UserGroup::Registered);

            if id == 0 {
                send_str(&cx, "Непонятный id").await;
            } else {
                let r = state
                    .0
                    .add_user(db_utils::models::User { id, group })
                    .await
                    .map(|_| format!("Добавил пользователя с id {}", id))
                    .unwrap_or_else(|e| {
                        format!(
                            "не получилось добавить пользователя. Ошибка: {}",
                            e.to_string()
                        )
                    });
                send_str(&cx, r.as_str()).await;
            }
        } else if let Some(del_user) = c.name("del_user").map(|m| m.as_str()) {
            let id = del_user
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or_default();
            if id == 0 {
                send_str(&cx, "Непонятный id").await;
            } else {
                let r = state
                    .0
                    .delete_user(id)
                    .await
                    .map(|_| format!("Удалил пользователя с id {}", id))
                    .unwrap_or_else(|e| {
                        format!(
                            "не получилось удалить пользователя. Ошибка: {}",
                            e.to_string()
                        )
                    });
                send_str(&cx, r.as_str()).await;
            }
        } else if let Some(_) = c.name("list_chats").map(|m| m.as_str()) {
            match state.0.list_chats().await {
                Ok(chats) => {
                    if chats.is_empty() {
                        send_str(&cx, "Пока не добавлено чатов. Используйте /add_chat id, чтобы добавить чат.").await;
                    }
                    for chat in chats {
                        send_str(&cx, chat.to_string().as_str()).await;
                    }
                }
                Err(e) => send_str(&cx, e.to_string().as_str()).await,
            }
        } else if let Some(add_user) = c.name("add_chat").map(|m| m.as_str()) {
            let id = add_user
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or_default();

            if id == 0 {
                send_str(&cx, "Непонятный id").await;
            } else {
                ALL_CHATS.write().await.insert(id);
                let r = state
                    .0
                    .add_chat(id)
                    .await
                    .map(|_| format!("Добавил чат с id {}", id))
                    .unwrap_or_else(|e| {
                        format!("Не получилось добавить чат. Ошибка: {}", e.to_string())
                    });
                send_str(&cx, r.as_str()).await;
            }
        } else if let Some(del_user) = c.name("del_chat").map(|m| m.as_str()) {
            let id = del_user
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<i64>()
                .unwrap_or_default();
            if id == 0 {
                send_str(&cx, "Непонятный id").await;
            } else {
                let r = state
                    .0
                    .delete_chat(id)
                    .await
                    .map(|_| format!("Удалил чат с id {}", id))
                    .unwrap_or_else(|e| {
                        format!("не получилось удалить чат. Ошибка: {}", e.to_string())
                    });
                send_str(&cx, r.as_str()).await;
            }
        } else if let Some(listdb) = c.name("listdb").map(|m| m.as_str()) {
            let mut it = listdb.split_whitespace();
            let date = it
                .nth(1)
                .map(|d| d.to_string())
                .unwrap_or(chrono::Local::today().format("%d.%m.%y").to_string());
            let zone = it.nth(0).unwrap_or("+03:00");

            let date = format!("{} 00:00:00.000 {}", date, zone);
            let date = chrono::DateTime::parse_from_str(date.as_str(), "%d.%m.%y %H:%M:%S%.3f %:z")
                .map(|d| d.with_timezone(&chrono::Utc));
            if let Ok(date) = date {
                let start = date;
                let end = start.checked_add_signed(chrono::Duration::days(1)).unwrap();
                match state
                    .0
                    .list_messages(vec![], vec![], Some(start), Some(end))
                    .await
                {
                    Ok(messages) => {
                        let mut msgs = BTreeMap::<String, Vec<db_utils::models::Message>>::new();
                        messages.iter().for_each(|m| {
                            m.regions
                                .iter()
                                .for_each(|r| msgs.entry(r.clone()).or_default().push(m.clone()))
                        });
                        for (region, messages) in msgs.iter().filter(|(r, _)| r.as_str() != "РФ")
                        {
                            send_str(&cx, format!("Регион: {}", region).as_str()).await;
                            send_messages(&cx, messages.clone(), true).await;
                        }
                        for messages in msgs.get(&"РФ".to_string()) {
                            send_str(&cx, format!("Регион: РФ").as_str()).await;
                            send_messages(&cx, messages.clone(), true).await;
                        }
                    }
                    Err(e) => send_str(&cx, e.to_string().as_str()).await,
                }
            } else {
                send_str(&cx, "Неправильная дата").await;
            }
        } else if let Some(deldb) = c.name("deldb").map(|m| m.as_str()) {
            let id = deldb
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<ObjectId>();
            if let Ok(id) = id {
                let r = state
                    .0
                    .delete_message(id)
                    .await
                    .map(|_| format!("Удалил сообщение с id {}", id))
                    .unwrap_or_else(|e| {
                        format!("Не получилось удалить сообщение. Ошибка: {}", e.to_string())
                    });
                send_str(&cx, r.as_str()).await;
            } else {
                send_str(&cx, "Непонятный id").await;
            }
        } else if let Some(cleandb) = c.name("cleandb").map(|m| m.as_str()) {
            let days = cleandb
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .parse::<i32>();
            if let Ok(days) = days {
                let before = chrono::Utc::now()
                    .checked_sub_signed(chrono::Duration::days(days as i64))
                    .unwrap();
                let r = state
                    .0
                    .delete_messages_period(None, Some(before))
                    .await
                    .map(|_| format!("Удалил все сообщения до {}", before))
                    .unwrap_or_else(|e| {
                        format!("Не получилось удалить сообщения. Ошибка: {}", e.to_string())
                    });
                send_str(&cx, r.as_str()).await;
            } else {
                send_str(&cx, "Непонятное количество дней").await;
            }
        } else if let Some(statdb) = c.name("statdb").map(|m| m.as_str()) {
            let zone = statdb.split_whitespace().nth(1).unwrap_or("+03:00");
            let hours = zone
                .split(':')
                .nth(0)
                .unwrap_or("+03")
                .parse::<i32>()
                .unwrap();
            let minuts = zone
                .split(':')
                .nth(1)
                .unwrap_or("00")
                .parse::<i32>()
                .unwrap();
            let secs = hours * 60 * 60 + hours.signum() * minuts * 60;
            let offset = chrono::FixedOffset::east(secs);

            let msg = match state.0.stat(offset).await {
                Ok(stat) => {
                    format!(
                        "Количество сообщений ({}).\n\
                    Сегодня\t- {}\n\
                    Вчера\t- {}\n\
                    Позавчера\t- {}\n\
                    Неделя\t- {}\n\
                    Месяц\t- {}\n\
                    Ранее\t- {}\n\
                    ",
                        zone,
                        stat.today,
                        stat.yesterday,
                        stat.before_yesterday,
                        stat.week,
                        stat.month,
                        stat.earlier,
                    )
                }
                Err(e) => {
                    format!("Не получилось выполнить команду. Ошибка: {}", e.to_string())
                }
            };

            send_str(&cx, msg.as_str()).await;
        }
        return next(state);
    } else if text.unwrap_or_default().starts_with('/') {
        match state.0.help().await {
            Ok(s) => send_str(&cx, s).await,
            Err(e) => send_str(&cx, e.to_string().as_str()).await,
        }
        return next(state);
    }

    let group = state.0.get_group().await;
    match group {
        Ok(g) if g == UserGroup::Unregistered => {
            send_str(&cx, text.unwrap_or_default()).await;
            return next(state);
        }
        Err(e) => {
            send_str(
                &cx,
                format!("Не получилось выполнить команду. Ошибка: {}", e.to_string()).as_str(),
            )
            .await;
            return next(state);
        }
        _ => {}
    }

    enum _Message {
        Message(BTreeMap<String, Vec<db_utils::models::Message>>),
        String(String),
    }
    let messages = match handle_private(&state, text.unwrap_or_default()).await {
        Ok(messages) => _Message::Message(messages),
        Err(e) => _Message::String(e.to_string()),
    };
    match messages {
        _Message::Message(messages) => {
            for (region, messages) in messages.iter().filter(|(r, _)| r.as_str() != "РФ") {
                send_str(&cx, format!("Регион: {}", region).as_str()).await;
                send_messages(&cx, messages.clone(), false).await;
            }
            for messages in messages.get(&"РФ".to_string()) {
                send_str(&cx, format!("Регион: РФ").as_str()).await;
                send_messages(&cx, messages.clone(), false).await;
            }
        }
        _Message::String(string) => {
            send_str(&cx, string.as_str()).await;
        }
    }
    next(state)
}

async fn handle_private(
    state: &Private,
    text: &str,
) -> Result<BTreeMap<String, Vec<db_utils::models::Message>>, Error> {
    let (regions, duration, tags) = match GET_REGEX.captures(text) {
        Some(c) => (
            c.name("regions").map(|r| r.as_str()),
            c.name("duration").map(|d| {
                match d.as_str().split_whitespace().next().unwrap().parse::<u64>() {
                    Ok(hours) => Ok(Duration::hours(hours as i64)),
                    Err(e) => Err(e),
                }
            }),
            c.name("tags").map(|t| t.as_str()),
        ),
        None => (None, None, None),
    };

    let regions = match regions {
        Some(regions) => match extract_regions(regions) {
            Regions::Regions(regions) => regions,
            Regions::BadRegion { region, matches } => {
                return Err(Error::BadRegion {
                    region: region.into(),
                    matches,
                });
            }
        },
        _ => return Err(Error::NoRegions),
    };

    let duration = match duration {
        Some(Ok(d)) => d,
        Some(Err(e)) => return Err(Error::DurationParseError(e)),
        None => Duration::hours(24),
    };

    let tags = match tags {
        Some(tags) => match extract_tags(tags) {
            Tags::Tags(tags) => tags,
            Tags::BadTag(t) => return Err(Error::BadTag(t.into())),
        },
        None => vec![],
    };

    let filter = db_utils::models::MessageFilter {
        duration,
        regions: regions.iter().map(|r| r.to_string()).collect(),
        tags: tags.iter().map(|t| t.to_string()).collect(),
    };

    let messages = db_utils::get_messages(&state.0.client, filter).await?;

    if messages.is_empty() {
        return Err(Error::NoMessages {
            regions: regions.iter().map(|&i| i.into()).collect(),
            duration,
            tags: tags.iter().map(|&i| i.into()).collect(),
        });
    }
    let mut res = BTreeMap::<String, Vec<db_utils::models::Message>>::new();
    messages.iter().for_each(|m| {
        m.regions
            .iter()
            .for_each(|r| res.entry(r.clone()).or_default().push(m.clone()))
    });

    Ok(res)
}
