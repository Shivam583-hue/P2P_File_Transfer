
# bittorrent-rs

[Preview](preview.png)

A BitTorrent-style P2P file transfer client built from scratch in Rust. No protocol libraries — raw TCP sockets, a custom binary protocol, SHA-256 chunk verification, a peer tracker, and concurrent multi-peer downloading.

Built as a Rust learning project covering async networking, systems concurrency, and P2P architecture.

---

## Architecture

```
[Seeder] ──── announce ────► [Tracker :9000] ◄──── query ──── [Leecher]
   ▲                                                                │
   └──────────────────────── chunks (TCP) ◄───────────────────────┘
```

Three separate processes:

- **Tracker** — directory server on port 9000. Holds a `DashMap<file_hash, Vec<SocketAddr>>`. Peers register themselves, leechers ask who has the file.
- **Seeder** — announces itself to the tracker, then serves chunk requests over TCP on port 8080.
- **Leecher** — queries the tracker for peers, connects to all of them, downloads chunks concurrently, verifies each chunk's SHA-256 hash, and reassembles the file.

---

## How it works

### Chunking
A file is split into 256-byte chunks. Each chunk is SHA-256 hashed. The hashes and file metadata are saved to a `manifest.json` file that both seeder and leecher use as the source of truth.

### Protocol
Custom binary protocol over TCP:

| Message | Format |
|---|---|
| Chunk request | `[0x01][4 bytes: chunk index]` |
| Announce to tracker | `[0x01][64 bytes: file hash][2 bytes: port]` |
| Query tracker | `[0x02][64 bytes: file hash][2 bytes: padding]` |
| Chunk response | `[4 bytes: length][N bytes: data]` |
| Tracker response | `[4 bytes: peer count][N * 6 bytes: IP + port]` |

### Concurrent downloading
The leecher spawns one `tokio` task per peer. Tasks share a `PieceManager` behind an `Arc<Mutex<>>` which tracks each chunk as `Needed → InFlight → Done`. Tasks pull work dynamically — no static chunk assignment. If a peer fails mid-transfer, its in-flight chunk is requeued and another peer can pick it up.

---

## Running it

Requires three terminals. Always start the tracker first.

**Terminal 1 — start the tracker**
```bash
cargo run --bin tracker
or 
make tracker
```

**Terminal 2 — chunk the file and start the seeder**
```bash
cargo run --bin client
or 
make run

cargo run --bin client -- seed
or 
make seeder
```

**Terminal 3 — run the leecher**
```bash
cargo run --bin client -- leech
or 
make leecher
```

Verify the output matches the original:
```bash
diff message.txt <reassembled_file>
```

To test with a larger file:
```bash
dd if=/dev/urandom bs=1M count=5 of=message.txt
cargo run --bin client
```

---

## Project structure

```
src/
├── main.rs              # CLI entrypoint (chunk / seed / leech)
├── lib.rs               # Seeder, leecher, networking, manifest, reassembly
├── piece_manager/
│   └── manager.rs       # PieceManager — chunk state machine
└── bin/
    └── tracker.rs       # Standalone tracker binary
```

---

## Crates used

| Crate | Purpose |
|---|---|
| `tokio` | Async runtime, TCP sockets, task spawning |
| `sha2` | SHA-256 chunk hashing |
| `serde` + `serde_json` | Manifest serialization |
| `dashmap` | Concurrent HashMap for tracker peer registry |
| `indicatif` | Download progress bar |

---
## Limitations

- Only supports IPv4 peers
- No peer discovery beyond tracker (no DHT)
- No upload throttling or rate control
- Single-file transfers only
- No persistence of peer state
