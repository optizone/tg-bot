use bson::oid::ObjectId;
use chrono::Duration;
use serde::{Deserialize, Serialize};

pub struct DbStat {
    pub today: usize,
    pub yesterday: usize,
    pub before_yesterday: usize,
    pub week: usize,
    pub month: usize,
    pub earlier: usize,
}

#[derive(
    Deserialize,
    Serialize,
    strum::AsRefStr,
    strum::Display,
    strum::EnumString,
    PartialEq,
    Eq,
    Debug,
    Clone,
)]
pub enum UserGroup {
    Admin,
    Registered,
    Unregistered,
}

#[derive(Deserialize, Serialize)]
pub struct User {
    pub id: i64,
    pub group: UserGroup,
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, Debug, Clone)]
pub struct Region {
    pub region: String,
    pub aliases: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewMessage {
    pub regions: Vec<String>,
    pub chat_id: i64,
    pub message_id: i32,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub _id: ObjectId,
    #[serde(
        deserialize_with = "bson::serde_helpers::chrono_datetime_as_bson_datetime::deserialize"
    )]
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub regions: Vec<String>,
    pub chat_id: i64,
    pub message_id: i32,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct InsertableMessage {
    #[serde(serialize_with = "bson::serde_helpers::chrono_datetime_as_bson_datetime::serialize")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub regions: Vec<String>,
    pub chat_id: i64,
    pub message_id: i32,
    pub tags: Vec<String>,
}

pub struct MessageFilter {
    pub user_id: i64,
    pub period: Option<(Duration, Duration)>,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
}
#[derive(Deserialize, Serialize, Default)]
pub struct LatestRequests {
    pub requests: Vec<LatestRequest>,
}

#[derive(Deserialize, Serialize)]
pub struct LatestRequest {
    pub region: String,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
