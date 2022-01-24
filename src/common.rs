use teloxide::{prelude::*, RequestError};

use crate::{db_utils, ALLIAS_REGIONS, ALL_REGIONS, ALL_TAGS};
use std::collections::HashSet;

#[derive(Debug)]
pub enum Regions<'t> {
    Country(Vec<&'t str>),
    Regions(Vec<&'t str>),
    BadRegion {
        region: &'t str,
        matches: Vec<&'static str>,
    },
}

pub fn extract_regions<'t>(regions: &'t str) -> Regions<'t> {
    let alias_regions = ALLIAS_REGIONS.read().unwrap();
    let mut res = Vec::new();
    for region in regions.split_whitespace() {
        if let Some(&reg) = alias_regions.get(region.to_lowercase().as_str()) {
            res.push(reg);
        } else if region.to_lowercase() == "страна" {
            return Regions::Country(ALL_REGIONS.read().unwrap().iter().copied().collect());
        } else {
            let mut it = alias_regions
                .iter()
                .filter(|(a, _)| a.starts_with(&region.to_lowercase()));
            let first = it.next();
            let mut other = it.map(|(_, r)| *r).collect::<HashSet<_>>();
            if !other.is_empty() && !other.iter().all(|r| r == first.unwrap_or((&"", &"")).1)
                || first.is_none()
            {
                first.map(|(_, r)| other.insert(*r));
                let mut matches = other.iter().copied().collect::<Vec<_>>();
                matches.sort_unstable();
                return Regions::BadRegion { region, matches };
            }
            res.push(*first.unwrap().1);
        }
    }
    Regions::Regions(res)
}

pub enum Tags<'t> {
    Tags(Vec<&'static str>),
    BadTag(&'t str),
}

pub fn extract_tags<'t>(tags: &'t str) -> Tags<'t> {
    let all_tags = ALL_TAGS
        .read()
        .map_err(|e| log::error!("Can't lock ALL_TAGS. Error: {}", e.to_string()))
        .unwrap();
    let mut res = Vec::new();
    for tag in tags.trim().split_whitespace() {
        match all_tags.get(tag.to_uppercase().as_str()) {
            Some(tag) => res.push(*tag),
            None => return Tags::BadTag(tag),
        }
    }
    Tags::Tags(res)
}

pub async fn send_str(cx: &TransitionIn<AutoSend<Bot>>, str: &str) {
    loop {
        match cx.answer(str).await {
            Ok(_) => break,
            Err(RequestError::RetryAfter(secs)) => {
                tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await
            }
            Err(e) => break log::error!("Error while sending a string: {}", e.to_string()),
        }
    }
}

pub async fn send_messages(
    cx: &TransitionIn<AutoSend<Bot>>,
    messages: Vec<db_utils::models::Message>,
    with_id: bool,
) {
    for message in messages {
        if with_id {
            send_str(cx, message._id.to_hex().as_str()).await;
        }
        loop {
            match cx
                .requester
                .forward_message(cx.chat_id(), message.chat_id, message.message_id)
                .await
            {
                Ok(_) => break,
                Err(RequestError::RetryAfter(secs)) => {
                    tokio::time::sleep(std::time::Duration::from_secs(secs as u64)).await
                }
                Err(e) => break log::error!("Error while forwarding a message: {}", e.to_string()),
            }
        }
    }
}
