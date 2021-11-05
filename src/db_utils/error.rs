use super::models::UserGroup;
use smartstring::{LazyCompact, SmartString};

type String = SmartString<LazyCompact>;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    DbError(#[from] mongodb::error::Error),
    #[error("Вы должны принадлежать группе {desired}, чтобы выполнить эту команду. Текущая группа: {current}")]
    PrivlegeError {
        desired: UserGroup,
        current: UserGroup,
    },
    #[error("Непонятный тег \"{0}\"")]
    BadTag(String),
    #[error("Непонятный регион \"{0}\"")]
    BadRegion(String),
}
