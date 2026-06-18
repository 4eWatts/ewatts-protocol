# eWatts Mining Guide

## Memory-Bound Proof-of-Work (MBPoW)

eWatts uses a memory-bound proof of work that replaces hash-based mining with DRAM bandwidth. Instead of SHA256 hashes (Bitcoin) or sequential memory-hard functions (Ethereum), MBPoW mines by performing random reads from a large DAG stored in RAM. The bottleneck is memory bandwidth, not hash power.

### Why This Matters

- **No ASICs** — DRAM is commodity hardware. No miner can get a 1000x advantage with custom chips.
- **Fair distribution** — Anyone with a standard server or PC can mine.
- **Stable cost floor** — DRAM manufacturing improves slowly (~7%/year), so mining cost doesn't collapse 30%/year like ASICs.

## Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| RAM | 4 GB | 16 GB |
| CPU | Any x86_64 | Any modern |
| OS | Linux, macOS, Windows | Linux |
| Storage | 100 MB | 1 GB |
| Network | Broadband | Low latency |

## Quick Start

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. Build eWatts

```bash
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol

# Testnet (fast mining, small DAG)
cargo build --release --features testnet

# Mainnet (slow mining, full DAG)
cargo build --release --no-default-features --features mainnet
```

### 3. Initialize

```bash
./target/release/ewatts-protocol init
```

This creates a `ewatts_data/` directory with the genesis state and wallet keys.

### 4. Run a Node

**Mining node** (generates new blocks):
```bash
./target/release/ewatts-protocol start --p2p --difficulty 100
```

**Light node** (follows the chain, doesn't mine):
```bash
./target/release/ewatts-protocol start --p2p --no-mine --bootstrap /ip4/<BOOTSTRAP_IP>/tcp/<PORT>/p2p/<PEER_ID>
```

**With custom ports:**
```bash
./target/release/ewatts-protocol start --p2p --p2p-port 9001 --dash-port 8081 --difficulty 100
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--p2p` | off | Enable P2P networking |
| `--p2p-port` | 0 (random) | P2P listen port |
| `--dash-port` | 8080 | Dashboard HTTP port |
| `--difficulty` | 100 | Mining difficulty (testnet: 10-1000) |
| `--no-mine` | off | Disable mining (light/follower node) |
| `--bootstrap` | — | Bootstrap peer address |

## Dashboard

Each node serves a real-time dashboard:

```
http://localhost:8080/dashboard-v3.html
```

### API Endpoints

- `GET /api/status` — Full node status (height, supply, UTXOs, blocks, peers)
- `GET /api/mempool` — Pending transactions
- `GET /api/balance/<pubkey_hex>` — Balance for a public key
- `GET /api/ring/pool` — UTXO ring pool
- `POST /api/submit_tx` — Submit a transaction

### Example Status Response

```json
{
  "height": 350,
  "supply": 100000000,
  "utxos": 351,
  "peers": 1,
  "mempool": 0,
  "blocks": [...]
}
```

## Testnet vs Mainnet

| Parameter | Testnet | Mainnet |
|-----------|---------|---------|
| Block time | 60s | 600s (10 min) |
| DAG size | 4 MB | 8 GB |
| Initial difficulty | 10-100 | Dynamic |
| Ramp-up | 100 blocks | 10,000 blocks |
| Founder lock | 500 blocks | 50,000 blocks |
| Genesis supply | 1M Ewatt (random key) | Empty state |

## Network

Testnet bootstrap nodes:

```
/ip4/178.104.193.51/tcp/9992/p2p/12D3KooWHUcxETatrssJzoq9xPqZu5ParFvdQigQwKkPcBLLZPKG
```

## Architecture

```
src/
├── main.rs        CLI · Dashboard · Mining
├── block.rs       Block structure · Merkle tree
├── proof.rs       MBPoW DAG mining algorithm
├── p2p.rs         libp2p networking · Gossipsub · Block sync
├── state.rs       UTXO set · Supply tracking
├── wallet.rs      Key generation · Private transactions
├── reward.rs      Emission formula · VR computation
├── chain.rs       Chain store · Reorg engine
├── mempool.rs     Transaction pool
├── store.rs       Disk persistence
└── constants.rs   Network parameters
```

## Docker

```bash
docker compose up -d
```

See `Dockerfile` and `docker-compose.yml`.

## Security

- All transactions are private by default (MLSAG ring signatures + stealth addresses)
- Pedersen commitments hide amounts
- Range proofs prevent inflation
- Coinbase outputs are locked during ramp-up (founder cannot spend early)
- DDoS protection: 30 req/min per IP on dashboard

## License

MIT
