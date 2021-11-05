use std::collections::{HashMap, HashSet};

use bson::{doc, Document};
use mongodb::Client;
use mongodb::{error::Result, IndexModel};

use crate::db_utils::{CHATS_COLLECTION_NAME, MESSAGES_COLLECTION_NAME};

use super::{DB_NAME, USERS_COLLECTION_NAME};

pub async fn validate_col<'c, 'i>(
    client: &Client,
    col_name: &'c str,
    index_builders: HashMap<&'i str, fn() -> IndexModel>,
) -> Result<()> {
    log::info!("Validating collection {}", col_name);
    log::info!("Checking for indexes");

    let db = client.database(DB_NAME);
    let col = db.collection::<Document>(col_name);
    let indecies = col
        .list_index_names()
        .await
        .map_err(|e| {
            log::error!("Can't access indecies: {}", e.to_string());
            e
        })?
        .into_iter()
        .collect::<HashSet<_>>();

    for (name, builder) in &index_builders {
        if !indecies.contains(*name) {
            log::info!("Creating index: {}", name);
            col.create_index(builder(), None).await?;
        } else {
            log::info!("Index {} found", name);
        }
    }

    log::info!("Collection {} is valid", USERS_COLLECTION_NAME);
    Ok(())
}

pub async fn validate_db(client: &Client) -> Result<()> {
    log::info!("Validating database {}", DB_NAME);

    let messages = {
        let mut h = HashMap::<_, fn() -> IndexModel>::with_capacity(4);
        h.insert(MESSAGES_INDEX_NAME, messages_index_build);
        h
    };

    let users = {
        let mut h = HashMap::<_, fn() -> IndexModel>::with_capacity(4);
        h.insert(ID_INDEX_NAME, id_index_build);
        h
    };

    let chats = {
        let mut h = HashMap::<_, fn() -> IndexModel>::with_capacity(4);
        h.insert(ID_INDEX_NAME, id_index_build);
        h
    };

    validate_col(client, MESSAGES_COLLECTION_NAME, messages).await?;
    validate_col(client, USERS_COLLECTION_NAME, users).await?;
    validate_col(client, CHATS_COLLECTION_NAME, chats).await?;

    log::info!("Database {} is valid", DB_NAME);

    Ok(())
}

const ID_INDEX_NAME: &str = "id_index";
const MESSAGES_INDEX_NAME: &str = "messages_index";

fn id_index_build() -> IndexModel {
    mongodb::IndexModel::builder()
        .keys(doc! { "id": 1 })
        .options(
            mongodb::options::IndexOptions::builder()
                .name(ID_INDEX_NAME.to_string())
                .unique(true)
                .build(),
        )
        .build()
}

fn messages_index_build() -> IndexModel {
    mongodb::IndexModel::builder()
        .keys(doc! {
            "region": 1,
            "tags": 1,
            "timestamp": 1
        })
        .options(
            mongodb::options::IndexOptions::builder()
                .name(ID_INDEX_NAME.to_string())
                .build(),
        )
        .build()
}
