use dashmap::DashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn process(
    mut socket: TcpStream,
    a: Arc<DashMap<String, Vec<SocketAddr>>>,
) -> io::Result<()> {
    let mut buf = [0u8; 67];

    if socket.read_exact(&mut buf).await.is_err() {
        return Ok(());
    }

    let message_type = buf[0];
    if message_type == 0x01 {
        let port = u16::from_be_bytes([buf[65], buf[66]]);

        let ip = socket.peer_addr()?.ip();

        let addr = SocketAddr::new(ip, port);

        let hash_bytes = &buf[1..65];
        let hash = String::from_utf8_lossy(hash_bytes).to_string();

        println!("Announce received. Hash {}", hash.clone());
        println!(
            "Tracker state: {:?}",
            a.iter().map(|e| e.key().clone()).collect::<Vec<_>>()
        );

        let mut entry = a.entry(hash).or_insert(Vec::new());

        if !entry.contains(&addr) {
            entry.push(addr);
        }
    } else if message_type == 0x02 {
        let hash_bytes = &buf[1..65];
        let hash = String::from_utf8_lossy(hash_bytes).to_string();

        println!("Query received. Hash{} ", hash.clone());
        println!(
            "Known hashes: {:?}",
            a.iter().map(|e| e.key().clone()).collect::<Vec<_>>()
        );
        let list_of_peers: Vec<SocketAddr> = match a.get(&hash) {
            Some(peers) => peers.clone(),
            None => Vec::new(),
        };

        let mut response = Vec::new();

        response.extend_from_slice(&(list_of_peers.len() as u32).to_be_bytes());

        for peer in list_of_peers {
            if let SocketAddr::V4(v4) = peer {
                let ip_bytes = v4.ip().octets();
                let port_bytes = v4.port().to_be_bytes();

                response.extend_from_slice(&ip_bytes);
                response.extend_from_slice(&port_bytes);
            }
        }

        socket.write_all(&response).await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:9000").await?;
    println!("Tracker running on port 9000");

    let map = DashMap::new();
    let a = Arc::new(map);

    loop {
        let (socket, _) = listener.accept().await?;
        let aclone = a.clone();
        tokio::spawn(async move {
            if let Err(e) = process(socket, aclone).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}
