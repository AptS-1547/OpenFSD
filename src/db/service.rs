use crate::db::entities::{client_whitelist, user};
use sea_orm::*;

/// Check if a client ID is whitelisted
pub async fn is_client_whitelisted(
    db: &DatabaseConnection,
    client_id: &str,
) -> Result<bool, DbErr> {
    let result = client_whitelist::Entity::find()
        .filter(client_whitelist::Column::ClientId.eq(client_id))
        .filter(client_whitelist::Column::Enabled.eq(true))
        .one(db)
        .await?;

    Ok(result.is_some())
}

/// Find user by network ID
pub async fn find_user_by_network_id(
    db: &DatabaseConnection,
    network_id: &str,
) -> Result<Option<user::Model>, DbErr> {
    user::Entity::find()
        .filter(user::Column::NetworkId.eq(network_id))
        .one(db)
        .await
}

/// Create a new user
pub async fn create_user(
    db: &DatabaseConnection,
    network_id: String,
    password_hash: String,
    real_name: String,
    atc_rating: i32,
    pilot_rating: i32,
) -> Result<user::Model, DbErr> {
    let now = chrono::Utc::now();
    let user = user::ActiveModel {
        network_id: Set(network_id),
        password_hash: Set(password_hash),
        real_name: Set(real_name),
        atc_rating: Set(atc_rating),
        pilot_rating: Set(pilot_rating),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    };

    user.insert(db).await
}

/// Add client to whitelist
pub async fn add_client_to_whitelist(
    db: &DatabaseConnection,
    client_id: String,
    client_name: String,
) -> Result<client_whitelist::Model, DbErr> {
    let whitelist_entry = client_whitelist::ActiveModel {
        client_id: Set(client_id),
        client_name: Set(client_name),
        enabled: Set(true),
        created_at: Set(chrono::Utc::now().into()),
        ..Default::default()
    };

    whitelist_entry.insert(db).await
}
