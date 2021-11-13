use chrono::Duration;
use smartstring::{LazyCompact, SmartString};
type String = SmartString<LazyCompact>;
use crate::ALL_TAGS;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Не указаны регионы")]
    NoRegions,
    #[error("Непонятная продолжительность")]
    DurationParseError(#[from] std::num::ParseIntError),
    #[error("Непонятный регион \"{region}\".\nСовпадения: {matches:?}")]
    BadRegion {
        region: String,
        matches: Vec<&'static str>,
    },
    #[error("По такому запросу нет сообщений.")]
    NoMessages {
        regions: Vec<String>,
        duration: Duration,
        tags: Vec<String>,
    },
    #[error("Непонятный тег \"{0}\". Допустимые теги: [ {} ]", 
        ALL_TAGS
            .read()
            .map_err(|e| log::error!("Can't lock ALL_TAGS. Error: {}", e.to_string()))
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .join(", "))]
    BadTag(String),
    #[error("{0}")]
    ServerError(#[from] ServerError),
    #[error("{0}")]
    DbError(#[from] crate::db_utils::error::Error),
}

#[derive(thiserror::Error, Debug, serde::Deserialize)]
#[error("Сервер вернул ошибку {error}")]
pub struct ServerError {
    pub error: String,
}
