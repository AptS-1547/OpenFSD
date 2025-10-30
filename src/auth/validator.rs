use crate::auth::password;
use crate::db::{entities::user, service};
use sea_orm::DatabaseConnection;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("Client not whitelisted: {0}")]
    ClientNotWhitelisted(String),
    #[error("User not found")]
    UserNotFound,
    #[error("Database error: {0}")]
    DatabaseError(#[from] sea_orm::DbErr),
    #[error("Password verification error")]
    PasswordError,
}

/// Validate client ID against whitelist
pub async fn validate_client_id(
    db: &DatabaseConnection,
    client_id: &str,
) -> Result<(), AuthError> {
    let is_whitelisted = service::is_client_whitelisted(db, client_id).await?;

    if !is_whitelisted {
        log::warn!("Client ID not whitelisted: {}", client_id);
        return Err(AuthError::ClientNotWhitelisted(client_id.to_string()));
    }

    Ok(())
}

/// Validate user login credentials
pub async fn validate_login(
    db: &DatabaseConnection,
    network_id: &str,
    password: &str,
) -> Result<user::Model, AuthError> {
    // Find user by network ID
    let user = service::find_user_by_network_id(db, network_id)
        .await?
        .ok_or(AuthError::UserNotFound)?;

    // Verify password
    let password_valid = password::verify_password(password, &user.password_hash)
        .map_err(|e| {
            log::error!("Password verification error: {}", e);
            AuthError::PasswordError
        })?;

    if !password_valid {
        log::warn!("Invalid password for user: {}", network_id);
        return Err(AuthError::InvalidCredentials);
    }

    log::info!("User {} successfully authenticated", network_id);
    Ok(user)
}
