use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use bson::oid::ObjectId;
use bson::{doc, Document};
use chrono::{DateTime, Duration, NaiveTime, Utc};
use futures::StreamExt;
use mongodb::options::{FindOptions, UpdateOptions};
use mongodb::Client;
use mongodb::{error::Result as DbResult, Cursor};
use serde::Deserialize;

use crate::db_utils::models::{DbStat, LatestRequests};
use crate::db_utils::{
    CHATS_COLLECTION_NAME, DB_NAME, REGIONS_COLLECTION_NAME, TAGS_COLLECTION_NAME,
    USERS_COLLECTION_NAME, USER_LATEST_REQUESTS_COLLECTION_NAME,
};

use super::models::{InsertableMessage, MessageFilter, NewMessage, UserGroup};
use super::{
    models::{Message, Region, User},
    MESSAGES_COLLECTION_NAME,
};

pub async fn migrate_chat(client: &Client, from_id: i64, to_id: i64) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<Document>(CHATS_COLLECTION_NAME)
        .update_many(
            bson::doc! {
                "id": from_id
            },
            bson::doc! {
                "$set": { "id": to_id }
            },
            None,
        )
        .await?;

    client
        .database(DB_NAME)
        .collection::<Document>(MESSAGES_COLLECTION_NAME)
        .update_many(
            bson::doc! {
                "chat_id": from_id,
            },
            bson::doc! {
                "$set": { "chat_id": to_id }
            },
            None,
        )
        .await?;

    Ok(())
}

pub async fn get_regions(client: &Client) -> DbResult<Vec<Region>> {
    let mut cursor = client
        .database(DB_NAME)
        .collection::<Region>(REGIONS_COLLECTION_NAME)
        .find(None, None)
        .await?;

    let mut res = Vec::new();
    while let Some(reg) = cursor.next().await {
        match reg {
            Ok(reg) => {
                res.push(reg);
            }
            Err(e) => {
                log::error!(target: "db_utils::get_regions", "Error accessing regions: {}", &e);
                return Err(e);
            }
        }
    }
    Ok(res)
}

pub async fn get_tags(client: &Client) -> DbResult<HashSet<String>> {
    #[derive(Deserialize)]
    struct TagDoc {
        tag: String,
    }
    let mut cursor = client
        .database(DB_NAME)
        .collection::<TagDoc>(TAGS_COLLECTION_NAME)
        .find(None, None)
        .await?;

    let mut res = HashSet::new();
    while let Some(reg) = cursor.next().await {
        match reg {
            Ok(tag) => {
                res.insert(tag.tag);
            }
            Err(e) => {
                log::error!(target: "db_utils::get_tags", "Error accessing tags: {}", &e);
                return Err(e);
            }
        }
    }
    Ok(res)
}

pub async fn get_chats(client: &Client) -> DbResult<HashSet<i64>> {
    #[derive(Deserialize)]
    struct Chat {
        id: i64,
    }
    Ok(client
        .database(DB_NAME)
        .collection::<Chat>(CHATS_COLLECTION_NAME)
        .find(None, None)
        .await?
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .map(|c| c.map(|c| c.id))
        .collect::<Result<HashSet<_>, _>>()?)
}

pub async fn insert_chat(client: &Client, id: i64) -> DbResult<()> {
    Ok(client
        .database(DB_NAME)
        .collection::<Document>(CHATS_COLLECTION_NAME)
        .insert_one(mongodb::bson::doc! { "id": id }, None)
        .await
        .map(|_| ())?)
}

pub async fn delete_chat(client: &Client, id: i64) -> DbResult<()> {
    Ok(client
        .database(DB_NAME)
        .collection::<Document>(CHATS_COLLECTION_NAME)
        .delete_one(mongodb::bson::doc! { "id": id }, None)
        .await
        .map(|_| ())?)
}

pub async fn list_users(client: &Client, groups: Vec<UserGroup>) -> DbResult<Cursor<User>> {
    let filter = if !groups.is_empty() {
        Some(mongodb::bson::doc! {
            "groups": {
                "$in": groups.iter().map(|g| g.as_ref()).collect::<Vec<&str>>()
            }
        })
    } else {
        None
    };

    client
        .database(DB_NAME)
        .collection::<User>(USERS_COLLECTION_NAME)
        .find(
            filter,
            FindOptions::builder()
                .sort(mongodb::bson::doc! {
                    "regions": 1,
                    "timestamp": 1,
                })
                .build(),
        )
        .await
}

pub async fn add_user(client: &Client, user: User) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<User>(USERS_COLLECTION_NAME)
        .insert_one(user, None)
        .await
        .map(|_| ())
}

pub async fn delete_user(client: &Client, id: i64) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<Document>(USERS_COLLECTION_NAME)
        .delete_one(
            mongodb::bson::doc! {
                "id": id
            },
            None,
        )
        .await
        .map(|_| ())
}

pub async fn get_user_group(client: &Client, id: i64) -> DbResult<UserGroup> {
    client
        .database(DB_NAME)
        .collection::<Document>(USERS_COLLECTION_NAME)
        .find_one(
            mongodb::bson::doc! {
                "id": id
            },
            None,
        )
        .await
        .map(|d| {
            d.map_or(UserGroup::Unregistered, |d| {
                d.get("group")
                    .map(|g| {
                        UserGroup::from_str(g.as_str().unwrap_or("Unregistered"))
                            .unwrap_or(UserGroup::Unregistered)
                    })
                    .unwrap_or(UserGroup::Unregistered)
            })
        })
}

pub async fn delete_message(client: &Client, id: ObjectId) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<Document>(MESSAGES_COLLECTION_NAME)
        .delete_one(
            mongodb::bson::doc! {
                "_id": id
            },
            None,
        )
        .await
        .map(|_| ())
}

pub async fn delete_messages_period(
    client: &Client,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> DbResult<()> {
    let filter = match (after, before) {
        (Some(a), Some(b)) => mongodb::bson::doc! {
            "timestamp": {
                "$gte": mongodb::bson::DateTime::from_chrono(a),
                "$lte": mongodb::bson::DateTime::from_chrono(b),
            }
        },
        (None, Some(b)) => mongodb::bson::doc! {
            "timestamp": {
                "$lte": mongodb::bson::DateTime::from_chrono(b),
            }
        },
        (Some(a), None) => mongodb::bson::doc! {
            "timestamp": {
                "$gte": mongodb::bson::DateTime::from_chrono(a),
            }
        },
        (None, None) => mongodb::bson::doc! {
            "timestamp": {
                "$lte": mongodb::bson::DateTime::now(),
            }
        },
    };
    client
        .database(DB_NAME)
        .collection::<Document>(MESSAGES_COLLECTION_NAME)
        .delete_many(filter, None)
        .await
        .map(|_| ())
}

pub async fn list_messages(
    client: &Client,
    regions: Vec<String>,
    tags: Vec<String>,
    after: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
) -> DbResult<Cursor<Message>> {
    let mut filter = match (after, before) {
        (Some(a), Some(b)) => mongodb::bson::doc! {
            "timestamp": {
                "$gte": mongodb::bson::DateTime::from_chrono(a),
                "$lte": mongodb::bson::DateTime::from_chrono(b),
            }
        },
        (None, Some(b)) => mongodb::bson::doc! {
            "timestamp": {
                "$lte": mongodb::bson::DateTime::from_chrono(b),
            }
        },
        (Some(a), None) => mongodb::bson::doc! {
            "timestamp": {
                "$gte": mongodb::bson::DateTime::from_chrono(a),
            }
        },
        (None, None) => mongodb::bson::doc! {
            "timestamp": {
                "$lte": mongodb::bson::DateTime::now(),
            }
        },
    };

    if !tags.is_empty() {
        filter.insert(
            "tags",
            mongodb::bson::doc! {
                "$in": tags
            },
        );
    }

    if !regions.is_empty() {
        filter.insert(
            "regions",
            mongodb::bson::doc! {
                "$in": regions
            },
        );
    }

    client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .find(
            filter,
            FindOptions::builder()
                .sort(mongodb::bson::doc! {
                    "regions": 1,
                    "timestamp": 1,
                })
                .build(),
        )
        .await
}

pub async fn stat(client: &Client, offset: chrono::offset::FixedOffset) -> DbResult<DbStat> {
    let secs = offset.local_minus_utc();
    let today = Utc::today()
        .and_time(NaiveTime::from_hms(0, 0, 0))
        .unwrap_or_else(Utc::now)
        .checked_sub_signed(chrono::Duration::seconds(secs as i64))
        .unwrap_or_else(Utc::now);
    let yesterday = {
        let a = today.checked_sub_signed(Duration::days(1)).unwrap();
        let b = today.clone();
        (a, b)
    };
    let before_yesterday = {
        let a = today.checked_sub_signed(Duration::days(2)).unwrap();
        let b = today.checked_sub_signed(Duration::days(1)).unwrap();
        (a, b)
    };
    let week = {
        let a = today.checked_sub_signed(Duration::days(7)).unwrap();
        let b = today.clone();
        (a, b)
    };
    let month = {
        let a = today.checked_sub_signed(Duration::days(30)).unwrap();
        let b = today.clone();
        (a, b)
    };
    let earlier = today.checked_sub_signed(Duration::days(30)).unwrap();
    let filter_today = mongodb::bson::doc! {
        "timestamp": {
            "$gte": mongodb::bson::DateTime::from_chrono(today),
        }
    };
    let filter_yesterday = mongodb::bson::doc! {
        "timestamp": {
            "$gte": mongodb::bson::DateTime::from_chrono(yesterday.0),
            "$lte": mongodb::bson::DateTime::from_chrono(yesterday.1),
        }
    };
    let filter_before_yesterday = mongodb::bson::doc! {
        "timestamp": {
            "$gte": mongodb::bson::DateTime::from_chrono(before_yesterday.0),
            "$lte": mongodb::bson::DateTime::from_chrono(before_yesterday.1),
        }
    };
    let filter_week = mongodb::bson::doc! {
        "timestamp": {
            "$gte": mongodb::bson::DateTime::from_chrono(week.0),
            "$lte": mongodb::bson::DateTime::from_chrono(week.1),
        }
    };
    let filter_month = mongodb::bson::doc! {
        "timestamp": {
            "$gte": mongodb::bson::DateTime::from_chrono(month.0),
            "$lte": mongodb::bson::DateTime::from_chrono(month.1),
        }
    };
    let filter_earlier = mongodb::bson::doc! {
        "timestamp": {
            "$lte": mongodb::bson::DateTime::from_chrono(earlier),
        }
    };

    let today = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_today, None)
        .await? as usize;
    let yesterday = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_yesterday, None)
        .await? as usize;
    let before_yesterday = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_before_yesterday, None)
        .await? as usize;
    let week = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_week, None)
        .await? as usize;
    let month = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_month, None)
        .await? as usize;
    let earlier = client
        .database(DB_NAME)
        .collection::<Message>(MESSAGES_COLLECTION_NAME)
        .count_documents(filter_earlier, None)
        .await? as usize;

    Ok(DbStat {
        today,
        yesterday,
        before_yesterday,
        week,
        month,
        earlier,
    })
}

pub async fn insert_messages(
    client: &Client,
    all_regions: &HashSet<&'static str>,
    all_tags: &HashSet<&'static str>,
    messages: Vec<NewMessage>,
) -> super::error::Result<()> {
    if let Some(bad) = messages
        .iter()
        .find_map(|m| m.regions.iter().find(|r| !all_regions.contains(r.as_str())))
    {
        return Err(super::error::Error::BadRegion(bad.into()));
    }

    if let Some(bad) = messages
        .iter()
        .find_map(|m| m.tags.iter().find(|t| !all_tags.contains(t.as_str())))
    {
        return Err(super::error::Error::BadTag(bad.into()));
    }

    Ok(client
        .database(DB_NAME)
        .collection::<InsertableMessage>(MESSAGES_COLLECTION_NAME)
        .insert_many(
            messages
                .into_iter()
                .map(|msg| InsertableMessage {
                    timestamp: Utc::now(),
                    regions: msg.regions,
                    tags: msg.tags,
                    message_id: msg.message_id,
                    chat_id: msg.chat_id,
                })
                .collect::<Vec<_>>(),
            None,
        )
        .await
        .map(|_| ())?)
}

pub async fn add_user_regions(client: &Client, id: i64, regions: Vec<String>) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<mongodb::bson::Document>(USERS_COLLECTION_NAME)
        .update_one(
            mongodb::bson::doc! { "id": id },
            mongodb::bson::doc! { "$push": { "allowed_regions": { "$each": regions } } },
            UpdateOptions::builder().upsert(true).build(),
        )
        .await?;
    Ok(())
}

pub async fn del_user_regions(client: &Client, id: i64, regions: Vec<String>) -> DbResult<()> {
    client
        .database(DB_NAME)
        .collection::<mongodb::bson::Document>(USERS_COLLECTION_NAME)
        .update_one(
            mongodb::bson::doc! { "id": id },
            mongodb::bson::doc! { "$pull": { "allowed_regions": { "$in": regions } } },
            UpdateOptions::builder().upsert(true).build(),
        )
        .await?;
    Ok(())
}

pub async fn get_messages(
    client: &Client,
    filter: MessageFilter,
) -> super::error::Result<Vec<Message>> {
    let last_time = client
        .database(DB_NAME)
        .collection::<LatestRequests>(USER_LATEST_REQUESTS_COLLECTION_NAME)
        .find_one(
            doc! {
                "id": filter.user_id
            },
            None,
        )
        .await?
        .unwrap_or_default()
        .requests
        .into_iter()
        .map(|r| (r.region, r.timestamp))
        .collect::<HashMap<_, _>>();

    #[derive(Deserialize, Default)]
    struct Allowed {
        pub allowed_regions: HashSet<String>,
    }

    let regions = client
        .database(DB_NAME)
        .collection::<Allowed>(USERS_COLLECTION_NAME)
        .find_one(
            mongodb::bson::doc! {
                "id": filter.user_id
            },
            None,
        )
        .await?
        .unwrap_or_default()
        .allowed_regions;
    let f_regions = filter.regions.into_iter().collect::<HashSet<_>>();
    let regions = regions.intersection(&f_regions).collect::<Vec<_>>();

    let now = Utc::now();
    let mut result = Vec::with_capacity(16);
    for (region, timestamp) in regions.iter().map(|r| {
        let today = *last_time.get(*r).unwrap_or(&Utc::today().and_hms(0, 0, 0));
        (r, today)
    }) {
        let (after, before) = {
            let now = Utc::now();
            if let Some((since, duration)) = filter.period {
                let after = now.checked_sub_signed(since).unwrap_or_else(|| {
                    log::error!(target: "db_utils::db::get_messages", "Can't calculate timestamp with duration {:?}", &since);
                    now
                });

                let before = after.checked_add_signed(duration).unwrap_or_else(|| {
                    log::error!(target: "db_utils::db::get_messages", "Can't calculate timestamp with duration {:?}", &duration);
                    now
                });

                (after, before)
            } else {
                (timestamp, Utc::now())
            }
        };

        let before: mongodb::bson::DateTime = before.into();
        let after: mongodb::bson::DateTime = after.into();

        let mut doc = doc! {
            "timestamp": {
                "$gte": after,
                "$lte": before
            },
            "regions": region
        };

        if !filter.tags.is_empty() {
            doc.insert(
                "tags",
                mongodb::bson::doc! {
                    "$in": filter.tags.clone()
                },
            );
        }

        let res = client
            .database(DB_NAME)
            .collection::<Message>(MESSAGES_COLLECTION_NAME)
            .find(doc, None)
            .await?
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map(|mut messages| {
                for message in &mut messages {
                    let regs = std::mem::replace(&mut message.regions, Vec::new());
                    message.regions = regs
                        .into_iter()
                        .filter(|region| regions.contains(&region))
                        .collect();
                }
                messages
            })?;
        result.extend(res);

        client
            .database(DB_NAME)
            .collection::<Document>(USER_LATEST_REQUESTS_COLLECTION_NAME)
            .update_one(
                doc! { "id": filter.user_id },
                doc! { "$pull": { "requests": { "region": region } } },
                None,
            )
            .await?;
        client
            .database(DB_NAME)
            .collection::<Document>(USER_LATEST_REQUESTS_COLLECTION_NAME)
            .update_one(
                doc! { "id": filter.user_id },
                doc! { "$push": { "requests": { "region": region, "timestamp": now } } },
                UpdateOptions::builder().upsert(true).build(),
            )
            .await?;
    }

    Ok(result)
}
