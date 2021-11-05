use std::{str::FromStr, time::Duration};

use crate::error::{Error, ServerError};

lazy_static::lazy_static! {
    static ref SERVER_URL: String = std::env::var("SERVER_URL").unwrap_or_else(|_| {
        log::warn!(r#"Variable SERVER_URL isn't set, defaulting to "http://localhost:8080""#);
        String::from("http://localhost:8080")
    });
}

#[derive(strum::EnumString, Clone, Copy, PartialEq, Eq)]
pub enum UserGroup {
    Admin,
    Registered,
    Unregistered,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Message {
    // pub timestamp: DateTime<Utc>,
    pub chat_id: i64,
    pub message_id: i32,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(serde::Deserialize, Debug)]
pub struct RegionWitAliases {
    pub region: String,
    pub aliases: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct MessageFilter<'r, 't> {
    pub duration: Duration,
    pub regions: Vec<&'r str>,
    pub tags: Vec<&'t str>,
}

pub async fn send_messages(messages: &[Message]) -> Result<(), Error> {
    reqwest::Client::new()
        .post(format!("{}/messages", SERVER_URL.as_str()))
        .json(messages)
        .send()
        .await?;
    Ok(())
}

pub async fn get_user_group(id: i32) -> Result<UserGroup, Error> {
    let s = reqwest::get(format!("{}/users/{}/group", SERVER_URL.as_str(), id))
        .await?
        .text()
        .await?;
    Ok(UserGroup::from_str(s.as_str()).unwrap_or(UserGroup::Unregistered))
}

pub async fn get_messages<'r, 't>(filter: MessageFilter<'r, 't>) -> Result<Vec<Message>, Error> {
    let resp = reqwest::Client::new()
        .get(format!("{}/messages", SERVER_URL.as_str()))
        .json(&filter)
        .send()
        .await?
        .text()
        .await?;
    let messages = match serde_json::from_str::<Vec<Message>>(&resp) {
        Ok(m) => m,
        Err(_) => {
            let err = serde_json::from_str::<ServerError>(&resp)?;
            return Err(Error::ServerError(err));
        }
    };
    Ok(messages)
}

pub async fn get_regions() -> Result<Vec<RegionWitAliases>, Error> {
    let r = reqwest::get(format!("{}/regions", SERVER_URL.as_str()))
        .await?
        .text()
        .await?;
    Ok(serde_json::from_str::<Vec<RegionWitAliases>>(r.as_str())?)
}

pub async fn get_tags() -> Result<Vec<String>, Error> {
    let r = reqwest::get(format!("{}/tags", SERVER_URL.as_str()))
        .await?
        .text()
        .await?;
    Ok(serde_json::from_str::<Vec<String>>(r.as_str())?)
}
