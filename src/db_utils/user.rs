use bson::oid::ObjectId;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use mongodb::Client;
use std::collections::HashSet;
use std::sync::Arc;

use super::models::Message;
use super::models::UserGroup;
use crate::db_utils::models::DbStat;

type Error = crate::db_utils::error::Error;
type Result<T> = std::result::Result<T, Error>;

#[derive(Clone)]
pub struct User {
    pub client: Arc<Client>,
    pub id: i64,
}

impl User {
    pub fn new(id: i64, client: Arc<Client>) -> Self {
        Self { id, client }
    }

    pub async fn get_group(&self) -> Result<UserGroup> {
        Ok(super::db::get_user_group(&self.client, self.id).await?)
    }

    async fn try_admin(&self) -> Result<()> {
        let group = self.get_group().await?;
        match group {
            UserGroup::Admin => Ok(()),
            current => Err(Error::PrivlegeError {
                desired: UserGroup::Admin,
                current,
            }),
        }
    }

    pub fn start(&self) -> Result<&'static str> {
        Ok("Hello world!")
    }

    pub async fn help(&self) -> Result<&'static str> {
        let group = super::db::get_user_group(&self.client, self.id).await?;
        match group {
            UserGroup::Admin => Ok("/list_users\n\
                /add_user <id> [Admin]\n\
                /del_user <id>\n\
                /list_chats\n\
                /add_chat <id>\n\
                /del_chat <id>\n\
                /listdb <DD.MM.YY> [OFFSET, по умолчанию \'+03:00\' (Мск)]\n\
                /deldb <id>\n\
                /cleandb <суток оставить>\n\
                /statdb [OFFSET, по умолчанию \'+03:00\' (Мск)]\n\
                Регионы [часов] [теги]"),
            UserGroup::Registered => Ok("Регионы [часов] [теги]"),
            UserGroup::Unregistered => Ok("Тестовый эхо-бот"),
        }
    }

    // pub async fn get_regions(&self) -> Result<Vec<Region>> {
    //     Ok(super::db::get_regions(&self.client).await?)
    // }

    // pub async fn get_tags(&self) -> Result<HashSet<String>> {
    //     Ok(super::db::get_tags(&self.client).await?)
    // }

    pub async fn list_users(&self, groups: Vec<UserGroup>) -> Result<Vec<super::models::User>> {
        self.try_admin().await?;
        Ok(super::db::list_users(&self.client, groups)
            .await?
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub async fn add_user(&self, user: super::models::User) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::add_user(&self.client, user).await?)
    }

    pub async fn list_chats(&self) -> Result<HashSet<i64>> {
        self.try_admin().await?;
        Ok(super::db::get_chats(&self.client).await?)
    }

    pub async fn add_chat(&self, id: i64) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::insert_chat(&self.client, id).await?)
    }

    pub async fn delete_chat(&self, id: i64) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::delete_chat(&self.client, id).await?)
    }

    pub async fn delete_user(&self, user_id: i64) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::delete_user(&self.client, user_id).await?)
    }

    pub async fn delete_message(&self, id: ObjectId) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::delete_message(&self.client, id).await?)
    }

    pub async fn delete_messages_period(
        &self,
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Result<()> {
        self.try_admin().await?;
        Ok(super::db::delete_messages_period(&self.client, after, before).await?)
    }

    pub async fn list_messages(
        &self,
        regions: Vec<String>,
        tags: Vec<String>,
        after: Option<DateTime<Utc>>,
        before: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>> {
        self.try_admin().await?;
        Ok(
            super::db::list_messages(&self.client, regions, tags, after, before)
                .await?
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<std::result::Result<Vec<_>, _>>()?,
        )
    }

    pub async fn stat(&self, offset: chrono::offset::FixedOffset) -> Result<DbStat> {
        self.try_admin().await?;
        Ok(super::db::stat(&self.client, offset).await?)
    }
}
