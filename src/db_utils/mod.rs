mod db;
pub mod error;
pub mod models;
pub mod user;
mod validate;

pub use db::{get_chats, get_messages, get_regions, get_tags, insert_messages, migrate_chat};
pub use validate::validate_db;

pub(self) const DB_NAME: &str = "messages_db";
pub(self) const MESSAGES_COLLECTION_NAME: &str = "messages";
pub(self) const TAGS_COLLECTION_NAME: &str = "tags";
pub(self) const REGIONS_COLLECTION_NAME: &str = "regions";
pub(self) const USERS_COLLECTION_NAME: &str = "users";
pub(self) const CHATS_COLLECTION_NAME: &str = "chats";
pub(self) const USER_LATEST_REQUESTS_COLLECTION_NAME: &str = "user_latest_requests";
