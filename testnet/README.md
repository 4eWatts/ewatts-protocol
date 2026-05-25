# eWatts Testnet

Phase 2 of the eWatts roadmap: a public multi-node testnet.

## Quick Start

### 1. Build

```bash
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol
cargo build --release --features testnet
```

Target: `./target/release/ewatts-protocol`

### 2. Initialize

```bash
./target/release/ewatts-protocol init
```

Creates `ewatts_data/` with a genesis block holding 1,000,000 Ewatt for:
- A random testnet genesis key (saved in `ewatts_data/genesis_key.seed`)
- A random miner key (saved in `ewatts_data/miner_key.seed`)

### 3. Run a bootstrap node

```bash
./target/release/ewatts-protocol start \
  --p2p \
  --p2p-port 9000 \
  --dash-port 8080 \
  --difficulty 100
```

This starts:
- A dashboard at `http://localhost:8080/dashboard-v3.html`
- A P2P node listening on `/ip4/0.0.0.0/tcp/9000`
- Continuous mining (1 block per ~60s on testnet)

### 4. Connect a peer

```bash
./target/release/ewatts-protocol start \
  --p2p \
  --p2p-port 9001 \
  --dash-port 8081 \
  --bootstrap /ip4/<BOOTSTRAP_IP>/tcp/9000/p2p/<BOOTSTRAP_PEER_ID> \
  --difficulty 100
```

Peer ID is printed on bootstrap node startup. Copy it from the logs.

## Architecture

```
                          ┌──────────────────┐
         ┌───────────────│  Bootstrap Node   │───────────────┐
         │               │  /ip4/.../tcp/9000 │               │
         │               └──────────────────┘               │
         │                                                   │
   ┌───────────┐                                     ┌───────────┐
   │  Peer A   │◄──── Gossip ──── libp2p ──────────►│  Peer B   │
   │  /tcp/9001│                                     │  /tcp/9002│
   └───────────┘                                     └───────────┘
         │                                                   │
   ┌───────────┐                                     ┌───────────┐
   │ Dashboard │                                     │ Dashboard │
   │ :8081     │                                     │ :8082     │
   └───────────┘                                     └───────────┘
```

Each node:
- Mines blocks independently (solo mining, no pool)
- Gossips new blocks via libp2p gossipsub (5s heartbeat)
- Requests missing blocks from peers via `/ewatts/block-sync/1`
- Serves a local dashboard with chain state, mempool, UTXOs

## Configuration

### Genesis

Testnet genesis is created at `init` time with a random key and 1M Ewatt. This lets anyone run their own testnet. For a shared testnet, all nodes must use the same genesis by bootstrapping from a seed node that has already initialized.

For a deterministic testnet genesis (all nodes identical):

1. Run `init` on the bootstrap node
2. Copy `ewatts_data/` to all peer machines
3. Start all nodes with `--p2p --bootstrap <addr>`

### Testnet Constants

| Parameter | Testnet | Mainnet |
|-----------|---------|---------|
| Block time | 60s | 600s |
| DAG size | 256 MB | 8 GB |
| Ramp-up blocks | 100 | 10,000 |
| Founder lock | 500 blocks | 50,000 blocks |
| Commit window | 43 blocks | 4,300 blocks |
| Difficulty | 100 | Dynamic |

## Docker

```bash
# Build
docker compose build

# Start bootstrap node
docker compose up -d bootstrap

# Start a peer
docker compose up -d peer1

# Scale peers
docker compose up -d --scale peer=3
```

See `Dockerfile` and `docker-compose.yml` in the project root.

## Dashboard

Each node serves a dashboard:

- `http://<node-ip>:<dash-port>/dashboard-v3.html` — Full UI
- `http://<node-ip>:<dash-port>/api/status` — Node status JSON
- `http://<node-ip>:<dash-port>/api/mempool` — Pending transactions
- `http://<node-ip>:<dash-port>/api/blocks` — Block list
- `http://<node-ip>:<dash-port>/api/balance/<pubkey_hex>` — Balance query
- `http://<node-ip>:<dash-port>/api/ring/pool` — UTXO ring pool
- `POST http://<node-ip>:<dash-port>/api/submit_tx` — Submit transaction

## Monitoring

### Health check endpoint

```bash
curl http://localhost:8080/api/status | jq .
```

Returns:
```json
{
  "height": 142,
  "supply": 1000420000,
  "utxos": 143,
  "vr": 500000000,
  "emission": 50000000,
  "mempool": 0,
  "peers": 2,
  "blocks": [...]
}
```

### Prometheus

An optional Prometheus endpoint is available on port 9091 when started with `--monitor`:

```bash
cargo run --release -- start --p2p --p2p-port 9000 --monitor
```

### Logging

Set `RUST_LOG=info` for normal operation, `RUST_LOG=debug` for verbose output:

```bash
RUST_LOG=debug ./target/release/ewatts-protocol start --p2p
```

## Networking

### Ports

| Port | Protocol | Purpose |
|------|----------|---------|
| 9000 | TCP | libp2p P2P (configurable via --p2p-port) |
| 8080 | TCP | Dashboard HTTP (configurable via --dash-port) |
| 9091 | TCP | Prometheus metrics (--monitor flag) |

### Firewall

```bash
# Allow P2P traffic
ufw allow 9000/tcp

# Allow dashboard (restrict to internal network)
ufw allow from 10.0.0.0/8 to any port 8080 proto tcp
```

For public nodes: restrict dashboard port to trusted IPs only. The P2P port must be open to all peers.

## Operations

### Backup

The `ewatts_data/` directory contains the full chain state:

```bash
# Backup
tar czf ewatts_backup_$(date +%Y%m%d).tar.gz ewatts_data/

# Restore
tar xzf ewatts_backup_20260525.tar.gz
```

### Reset testnet

```bash
rm -rf ewatts_data/
./target/release/ewatts-protocol init
```

### Upgrading

1. Stop the node
2. Backup `ewatts_data/`
3. Build new binary
4. Restart
