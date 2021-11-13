use crate::{common::*, db_utils};
use crate::{error::Error, Dialogue, ALLIAS_REGIONS, ALL_REGIONS, ALL_TAGS};
use mongodb::Client;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;

lazy_static::lazy_static! {
    static ref FINALIZE_REGEX: regex::Regex =
        regex::Regex::new(r"^(?P<regions>([\p{L}-]{2,}\s*)+)?\s*(?P<tags>(\p{L}\s+)*\p{L}$)?$").expect("Cant create a regex");
}

#[derive(Clone)]
pub struct Chat {
    client: Arc<Client>,
    n: i32,
    messages: Vec<db_utils::models::NewMessage>,
}

impl Chat {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            n: 0,
            messages: vec![],
        }
    }
}

#[teloxide(subtransition)]
async fn chat(
    mut state: Chat,
    cx: TransitionIn<AutoSend<Bot>>,
    _: String,
) -> TransitionOut<Dialogue> {
    let chat = cx.requester.get_chat(cx.chat_id()).await?;
    let text = cx.update.text();
    let (text, respond_to) = match handle_chat(&mut state, chat.id, text, cx.update.id).await {
        Ok(HandleChat::Saved {
            n_messages,
            regions,
            tags,
        }) => {
            state.n = 0;
            let regions = if regions.len() > 1 {
                format!("[{}]", regions.join(", "))
            } else {
                regions[0].to_string()
            };
            let tags = if tags.len() > 0 {
                format!(": [{}]", tags.join(", "))
            } else {
                String::new()
            };
            (
                Some(format!("Сохранено [{}]\n{}{}", n_messages, regions, tags)),
                None,
            )
        }
        Ok(HandleChat::Remembered(id)) => {
            state.n += 1;
            (Some(format!("Принял {}", state.n)), Some(id))
        }
        Ok(HandleChat::Ignored(id)) => (Some("Проигнорированно".to_string()), Some(id)),
        Err(e @ Error::BadRegion { .. }) => (Some(e.to_string()), None),
        Err(e @ Error::BadTag(_)) => (Some(e.to_string()), None),
        Err(e) => {
            log::error!(
                "Unreachable branch while handling chat message: {:?}. Error: {}",
                text,
                e.to_string()
            );
            (Some(e.to_string()), None)
        }
    };

    match (text, respond_to) {
        (Some(t), Some(_)) => {
            while let Err(teloxide::RequestError::RetryAfter(secs)) = cx.reply_to(&t).await {
                tokio::time::sleep(Duration::from_secs(secs as u64)).await;
            }
        }
        (Some(t), None) => {
            while let Err(teloxide::RequestError::RetryAfter(secs)) = cx.answer(&t).await {
                tokio::time::sleep(Duration::from_secs(secs as u64)).await;
            }
        }
        _ => unreachable!(),
    }

    next(state)
}

enum HandleChat<'r, 't> {
    Remembered(i32),
    Saved {
        n_messages: usize,
        regions: Vec<&'r str>,
        tags: Vec<&'t str>,
    },
    Ignored(i32),
}

async fn handle_chat<'t>(
    state: &mut Chat,
    id: i64,
    text: Option<&'t str>,
    message_id: i32,
) -> Result<HandleChat<'t, 't>, Error> {
    let (regions, tags) = match FINALIZE_REGEX.captures(text.unwrap_or_default()) {
        Some(c) => (
            c.name("regions").map(|r| r.as_str()),
            c.name("tags").map(|t| t.as_str()),
        ),
        None => (None, None),
    };

    let regions = match regions {
        Some(regions) => match extract_regions(regions) {
            Regions::Regions(regions) if !regions.is_empty() => Some(regions),
            Regions::Regions(_) => None,
            Regions::BadRegion { .. } => None,
        },
        _ => None,
    };

    let tags = match tags {
        Some(tags) => match (&regions, extract_tags(tags)) {
            (Some(_), Tags::Tags(tags)) => Some(tags),
            (Some(_), Tags::BadTag(t)) => return Err(Error::BadTag(t.into())),
            _ => None,
        },
        None => Some(vec![]),
    };

    if let Some(regions) = regions {
        if regions.iter().all(|region| {
            ALLIAS_REGIONS
                .read()
                .map_err(|e| log::error!("Can't lock ALIAS_REGIONS. Error: {}", e.to_string()))
                .unwrap()
                .contains_key(*region)
        }) {
            let messages = &mut state.messages;
            let n_messages = messages.len();
            if !messages.is_empty() {
                messages.iter_mut().for_each(|m| {
                    m.regions = regions.iter().map(|&r| r.into()).collect();
                    m.tags = tags
                        .clone()
                        .unwrap_or_default()
                        .iter()
                        .map(|&t| t.into())
                        .collect();
                });
                let all_regions = ALL_REGIONS
                    .read()
                    .map_err(|e| log::error!("Can't lock ALL_REGIONS. Error: {}", e.to_string()))
                    .unwrap()
                    .clone();
                let all_tags = ALL_TAGS
                    .read()
                    .map_err(|e| log::error!("Can't lock ALL_TAGS. Error: {}", e.to_string()))
                    .unwrap()
                    .clone();
                let r = db_utils::insert_messages(
                    &state.client,
                    &all_regions,
                    &all_tags,
                    messages.drain(..).collect(),
                )
                .await;
                messages.clear();
                r?;
            }
            return Ok(HandleChat::Saved {
                n_messages,
                regions,
                tags: tags.unwrap_or_default(),
            });
        }
    }

    let text = text.unwrap_or_default();
    if text.chars().count() < 20 && text != "" {
        if let Regions::BadRegion { region, matches } = extract_regions(text) {
            return Err(Error::BadRegion {
                region: region.into(),
                matches,
            });
        }
        return Ok(HandleChat::Ignored(message_id));
    }

    state.messages.push(db_utils::models::NewMessage {
        regions: vec![],
        chat_id: id,
        message_id,
        tags: vec![],
    });
    Ok(HandleChat::Remembered(message_id))
}
