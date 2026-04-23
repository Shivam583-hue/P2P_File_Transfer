use BitTorrent::Manifest;
use BitTorrent::hash_chunk;
use std::fs;
use std::io;

fn write_to_file(i: usize, chunk: &[u8]) -> io::Result<()> {
    let filename = format!("message.chunk{}", i);
    fs::write(filename, chunk)?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("seed") => BitTorrent::seeder().await.unwrap(),
        Some("leech") => BitTorrent::leecher().await.unwrap(),
        _ => {
            let file_path = "message.txt";
            let attr = fs::metadata("message.txt").expect("Error occured while reading metadata");

            let contents = fs::read(file_path).expect("Couldn't read file");

            let mut hash_store: Vec<[u8; 32]> = vec![];
            let mut i: usize = 0;

            for chunk in contents.chunks(256) {
                hash_store.push(hash_chunk(chunk));
                let _ = write_to_file(i, chunk);
                i = i + 1;
            }

            let file1 = Manifest {
                filename: "message.txt".to_string(),
                total_size: attr.len(),
                chunk_size: 256,
                hashes: hash_store.clone(),
                numberof_chunks: i,
            };

            let json = serde_json::to_string_pretty(&file1).unwrap();
            fs::write("message.manifest.json", json).unwrap(); // add this
        }
    }
}
