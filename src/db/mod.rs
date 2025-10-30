pub mod entities;
pub mod service;

use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};
use std::time::Duration;

/// Initialize database connection and run migrations
pub async fn init(database_url: &str) -> Result<DatabaseConnection, DbErr> {
    log::info!("Connecting to database: {}", database_url);

    let mut opt = ConnectOptions::new(database_url.to_owned());
    opt.max_connections(100)
        .min_connections(5)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_secs(8))
        .max_lifetime(Duration::from_secs(8))
        .sqlx_logging(true)
        .sqlx_logging_level(log::LevelFilter::Debug);

    let db = Database::connect(opt).await?;

    log::info!("Running database migrations...");
    Migrator::up(&db, None).await?;
    log::info!("Database migrations completed");

    Ok(db)
}
