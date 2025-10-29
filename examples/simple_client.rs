/// Simple FSD client example
/// 
/// This example demonstrates how to connect to an FSD server and send basic packets.
/// 
/// Usage: cargo run --example simple_client

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("FSD Simple Client Example");
    println!("=========================\n");

    // Connect to the FSD server
    let server_addr = "127.0.0.1:6809";
    println!("Connecting to {}...", server_addr);
    
    let stream = TcpStream::connect(server_addr).await?;
    println!("Connected!\n");

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Spawn a task to read responses from server
    let read_handle = tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    println!("Server closed connection");
                    break;
                }
                Ok(_) => {
                    print!("< {}", line);
                }
                Err(e) => {
                    eprintln!("Error reading from server: {}", e);
                    break;
                }
            }
        }
    });

    // Send client identification
    let callsign = "TEST123";
    let id_packet = format!("$ID{}:SERVER:69d7:Example Client:3:2:1234567:987654321\r\n", callsign);
    println!("> {}", id_packet.trim_end());
    writer.write_all(id_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit for server response
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send pilot login
    let login_packet = format!("#AP{}:SERVER:1234567:password:1:1:2:John Doe KJFK\r\n", callsign);
    println!("> {}", login_packet.trim_end());
    writer.write_all(login_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send a position update
    let pos_packet = format!("@N{}:1200:1:40.6413:-73.7781:5000:250:414141414:30\r\n", callsign);
    println!("> {}", pos_packet.trim_end());
    writer.write_all(pos_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send a text message
    let msg_packet = format!("#TM{}:*:Hello from the example client!\r\n", callsign);
    println!("> {}", msg_packet.trim_end());
    writer.write_all(msg_packet.as_bytes()).await?;
    writer.flush().await?;

    // Wait a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // Send logoff
    let logoff_packet = format!("#DP{}:1234567\r\n", callsign);
    println!("> {}", logoff_packet.trim_end());
    writer.write_all(logoff_packet.as_bytes()).await?;
    writer.flush().await?;

    println!("\nClosing connection...");
    drop(writer);
    
    // Wait for reader to finish
    let _ = tokio::time::timeout(tokio::time::Duration::from_secs(2), read_handle).await;

    println!("Disconnected.");
    Ok(())
}
