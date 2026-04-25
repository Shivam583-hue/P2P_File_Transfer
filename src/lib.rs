pub mod piece_manager;
use crate::piece_manager::ChunkState;
use crate::piece_manager::PieceManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::{fs, io, sync::Arc};
use tokio::sync::Mutex;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub file_hash: String,
    pub filename: String,
    pub total_size: u64,
    pub chunk_size: usize,
    pub numberof_chunks: usize,
    pub hashes: Vec<[u8; 32]>,
}

pub fn hash_chunk(chunk: &[u8]) -> [u8; 32] {
    let result = Sha256::digest(chunk);
    result.into()
}

fn deserialize_manifest(mani: &str) -> Result<Manifest, Box<dyn std::error::Error>> {
    let manifest: Manifest = serde_json::from_str(mani)?;
    Ok(manifest)
}

pub fn reassembly() -> Result<(), Box<dyn std::error::Error>> {
    let mani_str = fs::read_to_string("message.manifest.json")?;
    let manifest = deserialize_manifest(&mani_str)?;

    let mut chunk_store: Vec<u8> = Vec::new();

    for i in 0..manifest.numberof_chunks {
        let chunk_filename = format!("message.chunk{}", i);
        let chunk_data = fs::read(&chunk_filename)?;

        let hash = hash_chunk(&chunk_data);
        if hash != manifest.hashes[i] {
            return Err(format!("Hash mismatch for chunk {}", i).into());
        }

        chunk_store.extend(chunk_data);
    }

    fs::write(&manifest.filename, &chunk_store)?;
    Ok(())
}

async fn send_chunk(stream: &mut TcpStream, chunk: &[u8]) -> io::Result<()> {
    let length = chunk.len() as u32;

    stream.write_all(&length.to_be_bytes()).await?;
    stream.write_all(chunk).await?;

    Ok(())
}

async fn read_response(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;

    let length = u32::from_be_bytes(len_buf) as usize;

    let mut data = vec![0u8; length];
    stream.read_exact(&mut data).await?;

    Ok(data)
}

async fn send_request(stream: &mut TcpStream, chunk_index: u32) -> io::Result<()> {
    let mut buf = [0u8; 5];

    buf[0] = 0x01;
    buf[1..5].copy_from_slice(&chunk_index.to_be_bytes());

    stream.write_all(&buf).await?;
    Ok(())
}

async fn process(mut socket: TcpStream, chunks: Arc<Vec<Vec<u8>>>) -> io::Result<()> {
    let mut buf = [0u8; 5];

    loop {
        if socket.read_exact(&mut buf).await.is_err() {
            break;
        }

        let message_type = buf[0];
        let chunk_index = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;

        if message_type != 0x01 {
            continue;
        }

        if chunk_index >= chunks.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "bad index"));
        }

        let chunk = &chunks[chunk_index];
        send_chunk(&mut socket, chunk).await?;
    }

    Ok(())
}

pub async fn seeder() -> io::Result<()> {
    let mani_str = fs::read_to_string("message.manifest.json")?;
    let manifest = deserialize_manifest(&mani_str).unwrap();

    let mut chunk_vec = Vec::new();
    for i in 0..manifest.numberof_chunks {
        let filename = format!("message.chunk{}", i);
        let data = fs::read(filename)?;
        chunk_vec.push(data);
    }

    let chunks = Arc::new(chunk_vec);

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("Seeder running on 127.0.0.1:8080");

    let mut stream = TcpStream::connect("127.0.0.1:9000").await?;

    let mut buf = [0u8; 67];
    buf[0] = 0x01;

    buf[1..65].copy_from_slice(manifest.file_hash.as_bytes());
    buf[65..67].copy_from_slice(&8080u16.to_be_bytes());

    stream.write_all(&buf).await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let chunks_clone = chunks.clone();

        tokio::spawn(async move {
            if let Err(e) = process(socket, chunks_clone).await {
                eprintln!("Connection error: {}", e);
            }
        });
    }
}

pub async fn leecher() -> io::Result<()> {
    let mani_str = fs::read_to_string("message.manifest.json")?;
    let manifest = deserialize_manifest(&mani_str).unwrap();

    let mut tracker_stream = TcpStream::connect("127.0.0.1:9000").await?;
    let mut buf = [0u8; 67];
    buf[0] = 0x02;

    buf[1..65].copy_from_slice(manifest.file_hash.as_bytes());
    buf[65..67].copy_from_slice(&8080u16.to_be_bytes());

    tracker_stream.write_all(&buf).await?;

    let mut len_buf = [0u8; 4];
    tracker_stream.read_exact(&mut len_buf).await?;
    let number_of_peers = u32::from_be_bytes(len_buf);

    let total_bytes = number_of_peers as usize * 6;
    let mut peers_buf = vec![0u8; total_bytes];

    tracker_stream.read_exact(&mut peers_buf).await?;

    let mut peers = Vec::new();

    for chunk in peers_buf.chunks_exact(6) {
        let ip = Ipv4Addr::new(chunk[0], chunk[1], chunk[2], chunk[3]);
        let port = u16::from_be_bytes([chunk[4], chunk[5]]);
        let addr = SocketAddr::new(IpAddr::V4(ip), port);

        peers.push(addr);
    }
    if peers.is_empty() {
        return Err(io::Error::new(io::ErrorKind::Other, "no peers found"));
    }

    drop(tracker_stream);
    println!("Peers: {:?}", peers);

    let chunks: Arc<Mutex<Vec<Option<Vec<u8>>>>> =
        Arc::new(Mutex::new(vec![None; manifest.numberof_chunks]));
    let mut handles = Vec::new();

    let manager = Arc::new(Mutex::new(PieceManager {
        states: vec![ChunkState::Needed; manifest.numberof_chunks],
    }));

    for peer in peers.iter() {
        let peer_addr = *peer;
        let chunks_clone = chunks.clone();
        let manifest_clone = manifest.clone();

        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            let mut stream = match TcpStream::connect(peer_addr).await {
                Ok(s) => s,
                Err(_) => return,
            };
            loop {
                let maybe_chunk = {
                    let mut mgr = manager_clone.lock().await;
                    mgr.next_chunk()
                };

                let i = match maybe_chunk {
                    Some(i) => i,
                    None => break,
                };

                if send_request(&mut stream, i as u32).await.is_err() {
                    let mut mgr = manager_clone.lock().await;
                    mgr.requeue(i);
                    break;
                }

                let chunk = match read_response(&mut stream).await {
                    Ok(c) => c,
                    Err(_) => {
                        let mut mgr = manager_clone.lock().await;
                        mgr.requeue(i);
                        break;
                    }
                };

                if hash_chunk(&chunk) != manifest_clone.hashes[i] {
                    let mut mgr = manager_clone.lock().await;
                    mgr.requeue(i);
                    continue;
                }

                {
                    let mut lock = chunks_clone.lock().await;
                    lock[i] = Some(chunk);
                }

                {
                    let mut mgr = manager_clone.lock().await;
                    mgr.complete(i);
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        if let Err(e) = h.await {
            eprintln!("Task failed: {:?}", e);
        }
    }
    let done = {
        let mgr = manager.lock().await;
        mgr.is_done()
    };

    if !done {
        return Err(io::Error::new(io::ErrorKind::Other, "download incomplete"));
    }

    let lock = chunks.lock().await;

    let mut final_data = Vec::new();

    for chunk in lock.iter() {
        final_data.extend(chunk.as_ref().unwrap());
    }

    fs::write(&manifest.filename, final_data)?;

    println!("Download + reassembly complete");

    Ok(())
}
