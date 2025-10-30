/// OpenFSD Admin Tool
///
/// Utility for managing OpenFSD database users and configuration
use openfsd::{auth, db};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════╗");
    println!("║      OpenFSD Admin Tool v0.1.0         ║");
    println!("╚════════════════════════════════════════╝\n");

    // Get database URL
    print!("数据库 URL [sqlite://openfsd.db]: ");
    io::stdout().flush()?;
    let mut db_url = String::new();
    io::stdin().read_line(&mut db_url)?;
    let db_url = db_url.trim();
    let db_url = if db_url.is_empty() {
        "sqlite://openfsd.db"
    } else {
        db_url
    };

    // Connect to database
    println!("\n🔌 连接数据库: {}", db_url);
    let db_conn = db::init(db_url).await?;
    println!("✅ 数据库连接成功！\n");

    // Main menu
    loop {
        println!("\n请选择操作:");
        println!("  1. 添加新用户");
        println!("  2. 列出所有用户");
        println!("  3. 添加客户端到白名单");
        println!("  0. 退出");
        print!("\n> ");
        io::stdout().flush()?;

        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => add_user(&db_conn).await?,
            "2" => list_users(&db_conn).await?,
            "3" => add_client_to_whitelist(&db_conn).await?,
            "0" => break,
            _ => println!("❌ 无效选择"),
        }
    }

    println!("\n👋 再见!");
    Ok(())
}

async fn add_user(db: &sea_orm::DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== 添加新用户 ===");

    print!("Network ID (VATSIM CID/IVAO VID): ");
    io::stdout().flush()?;
    let mut network_id = String::new();
    io::stdin().read_line(&mut network_id)?;
    let network_id = network_id.trim().to_string();

    print!("密码: ");
    io::stdout().flush()?;
    let mut password = String::new();
    io::stdin().read_line(&mut password)?;
    let password = password.trim();

    print!("真实姓名: ");
    io::stdout().flush()?;
    let mut real_name = String::new();
    io::stdin().read_line(&mut real_name)?;
    let real_name = real_name.trim().to_string();

    print!("ATC 等级 (1-12) [1]: ");
    io::stdout().flush()?;
    let mut atc_rating_str = String::new();
    io::stdin().read_line(&mut atc_rating_str)?;
    let atc_rating: i32 = atc_rating_str.trim().parse().unwrap_or(1);

    print!("飞行员等级 (1-11) [1]: ");
    io::stdout().flush()?;
    let mut pilot_rating_str = String::new();
    io::stdin().read_line(&mut pilot_rating_str)?;
    let pilot_rating: i32 = pilot_rating_str.trim().parse().unwrap_or(1);

    // Hash password
    println!("\n🔐 Hash 密码...");
    let password_hash = auth::password::hash_password(password)
        .map_err(|e| format!("Password hash error: {}", e))?;

    // Create user
    println!("💾 创建用户...");
    let user = db::service::create_user(
        db,
        network_id.clone(),
        password_hash,
        real_name,
        atc_rating,
        pilot_rating,
    )
    .await?;

    println!("\n✅ 用户创建成功！");
    println!("   Network ID: {}", user.network_id);
    println!("   真实姓名: {}", user.real_name);
    println!("   ATC 等级: {}", user.atc_rating);
    println!("   飞行员等级: {}", user.pilot_rating);

    Ok(())
}

async fn list_users(db: &sea_orm::DatabaseConnection) -> Result<(), Box<dyn std::error::Error>> {
    use sea_orm::EntityTrait;

    println!("\n=== 用户列表 ===\n");

    let users = db::entities::user::Entity::find().all(db).await?;

    if users.is_empty() {
        println!("📭 暂无用户");
    } else {
        for user in users {
            println!("📋 Network ID: {}", user.network_id);
            println!("   姓名: {}", user.real_name);
            println!("   ATC 等级: {} | 飞行员等级: {}", user.atc_rating, user.pilot_rating);
            println!("   创建时间: {}", user.created_at);
            println!();
        }
    }

    Ok(())
}

async fn add_client_to_whitelist(
    db: &sea_orm::DatabaseConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n=== 添加客户端到白名单 ===");

    print!("Client ID (4字符，如 69d7): ");
    io::stdout().flush()?;
    let mut client_id = String::new();
    io::stdin().read_line(&mut client_id)?;
    let client_id = client_id.trim().to_string();

    print!("Client 名称 (如 EuroScope 3.2): ");
    io::stdout().flush()?;
    let mut client_name = String::new();
    io::stdin().read_line(&mut client_name)?;
    let client_name = client_name.trim().to_string();

    // Add to whitelist
    println!("\n💾 添加到白名单...");
    let entry = db::service::add_client_to_whitelist(db, client_id.clone(), client_name).await?;

    println!("\n✅ 客户端已添加到白名单！");
    println!("   Client ID: {}", entry.client_id);
    println!("   Client 名称: {}", entry.client_name);

    Ok(())
}
