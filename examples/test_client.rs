/// Interactive FSD test client
///
/// This is a more feature-rich test client that allows interactive testing
/// of the FSD server with various commands and scenarios.
///
/// Usage: cargo run --example test_client
use std::io::{self, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

const DEFAULT_CALLSIGN: &str = "TEST123";
const DEFAULT_CID: &str = "1234567";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔════════════════════════════════════════╗");
    println!("║   OpenFSD Interactive Test Client     ║");
    println!("╚════════════════════════════════════════╝\n");

    // Connect to the FSD server
    let server_addr = "127.0.0.1:6809";
    println!("🔌 Connecting to {}...", server_addr);

    let stream = TcpStream::connect(server_addr).await?;
    println!("✅ Connected!\n");

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let (_tx, mut rx) = mpsc::channel::<String>(100);

    // Spawn a task to read responses from server
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("\n⚠️  Server closed connection");
                    break;
                }
                Ok(_) => {
                    print!("📥 {}", line);
                    io::stdout().flush().unwrap();
                }
                Err(e) => {
                    eprintln!("\n❌ Error reading from server: {}", e);
                    break;
                }
            }
        }
    });

    // Main command loop
    let mut callsign = DEFAULT_CALLSIGN.to_string();
    let mut logged_in = false;

    print_help();

    loop {
        print!("\n> ");
        io::stdout().flush()?;

        tokio::select! {
            // Handle incoming messages from server
            Some(_) = rx.recv() => {
                // Messages are printed by the reader task
            }

            // Handle user input
            input = tokio::task::spawn_blocking(|| {
                let mut buffer = String::new();
                io::stdin().read_line(&mut buffer).ok().map(|_| buffer)
            }) => {
                if let Ok(Some(input)) = input {
                    let input = input.trim();

                    if input.is_empty() {
                        continue;
                    }

                    match input.split_whitespace().next().unwrap_or("") {
                        "help" | "h" => {
                            print_help();
                        }
                        "quit" | "q" | "exit" => {
                            println!("👋 Disconnecting...");
                            if logged_in {
                                let logoff = format!("#DP{}:{}\r\n", callsign, DEFAULT_CID);
                                let _ = writer.write_all(logoff.as_bytes()).await;
                                let _ = writer.flush().await;
                            }
                            break;
                        }
                        "id" => {
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            if parts.len() > 1 {
                                callsign = parts[1].to_string();
                            }
                            send_identification(&mut writer, &callsign).await?;
                        }
                        "login" => {
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            let client_type = parts.get(1).unwrap_or(&"pilot");
                            send_login(&mut writer, &callsign, client_type).await?;
                            logged_in = true;
                        }
                        "logoff" => {
                            send_logoff(&mut writer, &callsign).await?;
                            logged_in = false;
                        }
                        "pos" => {
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            let lat = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(40.6413);
                            let lon = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(-73.7781);
                            let alt = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(5000);
                            send_position(&mut writer, &callsign, lat, lon, alt).await?;
                        }
                        "msg" => {
                            let parts: Vec<&str> = input.splitn(3, ' ').collect();
                            let to = parts.get(1).unwrap_or(&"*");
                            let message = parts.get(2).unwrap_or(&"Test message");
                            send_message(&mut writer, &callsign, to, message).await?;
                        }
                        "fp" => {
                            send_flight_plan(&mut writer, &callsign).await?;
                        }
                        "metar" => {
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            let icao = parts.get(1).unwrap_or(&"KJFK");
                            send_metar_request(&mut writer, &callsign, icao).await?;
                        }
                        "caps" => {
                            send_caps_response(&mut writer, &callsign).await?;
                        }
                        "rn" => {
                            let parts: Vec<&str> = input.split_whitespace().collect();
                            let target = parts.get(1).unwrap_or(&"*");
                            send_realname_request(&mut writer, &callsign, target).await?;
                        }
                        "raw" => {
                            let raw_packet = input.strip_prefix("raw ").unwrap_or("");
                            if !raw_packet.is_empty() {
                                let packet = format!("{}\r\n", raw_packet);
                                println!("📤 {}", packet.trim_end());
                                writer.write_all(packet.as_bytes()).await?;
                                writer.flush().await?;
                            }
                        }
                        "test" => {
                            println!("🧪 Running automated test sequence...\n");
                            run_test_sequence(&mut writer, &callsign).await?;
                            logged_in = true;
                        }
                        _ => {
                            println!("❓ Unknown command. Type 'help' for available commands.");
                        }
                    }
                }
            }
        }
    }

    drop(writer);
    println!("✅ Disconnected.");
    Ok(())
}

fn print_help() {
    println!("\n📖 Available Commands:");
    println!("  help, h              - Show this help");
    println!("  id [callsign]        - Send identification (default: TEST123)");
    println!("  login [pilot|atc]    - Login as pilot or ATC (default: pilot)");
    println!("  logoff               - Send logoff");
    println!("  pos [lat] [lon] [alt]- Send position update (default: JFK)");
    println!("  msg [to] [text]      - Send text message (default: broadcast)");
    println!("  fp                   - Send sample flight plan");
    println!("  metar [icao]         - Request METAR (default: KJFK)");
    println!("  caps                 - Send capabilities response");
    println!("  rn [callsign]        - Request real name");
    println!("  raw [packet]         - Send raw FSD packet");
    println!("  test                 - Run automated test sequence");
    println!("  quit, q, exit        - Disconnect and exit");
}

async fn send_identification(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!(
        "$ID{}:SERVER:69d7:OpenFSD Test Client:3:2:{}:987654321\r\n",
        callsign, DEFAULT_CID
    );
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_login(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    client_type: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = match client_type {
        "atc" | "ATC" => {
            // #AA(callsign):SERVER:(full name):(network ID):(password):(rating):(protocol version)
            format!(
                "#AA{}:SERVER:Test Controller:{}:password:5:100\r\n",
                callsign, DEFAULT_CID
            )
        }
        _ => {
            // #AP(callsign):SERVER:(network ID):(password):(rating):(protocol version):(num2):(full name ICAO)
            format!(
                "#AP{}:SERVER:{}:password:1:100:2:Test Pilot KJFK\r\n",
                callsign, DEFAULT_CID
            )
        }
    };
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_logoff(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("#DP{}:{}\r\n", callsign, DEFAULT_CID);
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_position(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    lat: f64,
    lon: f64,
    alt: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!(
        "@N{}:1200:1:{}:{}:{}:250:414141414:30\r\n",
        callsign, lat, lon, alt
    );
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_message(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    to: &str,
    message: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("#TM{}:{}:{}\r\n", callsign, to, message);
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_flight_plan(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!(
        "#FP{}:*:V:B738:420:KJFK:1200:1200:35000:KLAX:03:30:02:45:F:Remarks here\r\n",
        callsign
    );
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_metar_request(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    icao: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("$AX{}:SERVER:METAR:{}\r\n", callsign, icao);
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_caps_response(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!(
        "$CR{}:SERVER:CAPS:ATCINFO=1:MODELDESC=1:ACCONFIG=1\r\n",
        callsign
    );
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn send_realname_request(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
    target: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let packet = format!("$CQ{}:{}:RN\r\n", callsign, target);
    println!("📤 {}", packet.trim_end());
    writer.write_all(packet.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn run_test_sequence(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    callsign: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("1️⃣  Sending identification...");
    send_identification(writer, callsign).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n2️⃣  Logging in as pilot...");
    send_login(writer, callsign, "pilot").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n3️⃣  Sending position update...");
    send_position(writer, callsign, 40.6413, -73.7781, 5000).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n4️⃣  Sending broadcast message...");
    send_message(writer, callsign, "*", "Hello from test client!").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n5️⃣  Filing flight plan...");
    send_flight_plan(writer, callsign).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n6️⃣  Requesting METAR...");
    send_metar_request(writer, callsign, "KJFK").await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    println!("\n✅ Test sequence completed!");
    Ok(())
}
