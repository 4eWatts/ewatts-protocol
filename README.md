# eWatts Protocol

**Energy from memory, not machines.**

eWatts is a digital currency secured by DRAM bandwidth. Mining is memory-bound proof of work — the bottleneck is RAM speed, not hash power. No ASICs. No staking. No governance. Private by default.

| Status | Value |
|--------|-------|
| Release | **v7** (25/mai/2026) |
| Testnet | Live (84/84 tests, multi-node P2P sync) |
| P2P | Multi-node connected, gossip sync + block request/response |
| Mining | MBPoW — DRAM bandwidth, --no-mine for light nodes |
| Privacy | MLSAG ring sigs, stealth addresses, Pedersen commitments |
| P2PKH | pubkey_hash [20] + revealed_pubkey (quantum defense at rest) |
| Governance | Zero pre-mine, no admin keys, 95% miner/node threshold |
| Implementation | Rust, 7,872 LOC (21 modules) |
| Network | Testnet: hash power, Mainnet: coming soon |
| License | MIT |
| Domain | [ewatts.org](https://ewatts.org) |

---

## Quick Start

```bash
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol
cargo build --release

# Initialize state
./target/release/ewatts-protocol init

# Mine a block
./target/release/ewatts-protocol mine

# Start dashboard
./target/release/ewatts-protocol dash
```

Full guide: [MINING.md](MINING.md)

---

## Architecture

```
src/
├── main.rs         CLI · Dashboard HTTP · Mining orchestrator
├── privacy.rs      Stealth addresses · MLSAG ring sigs · Pedersen commitments · Range proofs
├── block.rs        Block structure · Merkle tree · MlsagData serialization
├── state.rs        UTXO set · MLSAG verification · Supply tracking
├── mempool.rs      Transaction pool · Broadcast endpoint · Range proof validation
├── wallet.rs       Key generation · UTXO scanning · Private transaction construction
├── reward.rs       Emission formula · VR computation · Ramp-up cap · Founder lock
├── proof.rs        MBPoW DAG mining algorithm
├── p2p.rs          libp2p gossip · Block sync
├── commitment.rs   Bandwidth commitment · Efficiency computation
├── store.rs        Disk persistence
└── vr.rs           Value of Resource computation
```

## Privacy

All transactions are private by default:

| Primitive | Implementation | Test |
|-----------|---------------|------|
| **MLSAG ring signatures** | Multi-layered, ring size 11, Ristretto255 | `test_mlsag_roundtrip`, `test_mlsag_multi_layer`, `test_mlsag_wrong_msg_fails` |
| **Stealth addresses** | One-time destinations, spend+view key model | `test_stealth_address_roundtrip` |
| **Pedersen commitments** | `C = a*G + v*H`, homomorphic | `test_pedersen_commitment`, `test_pedersen_homomorphic` |
| **Range proofs** | Bit-decomposition with MLSAG 1-of-2 | `test_range_proof` |

Key images prevent double-spending without linking transactions. Ephemeral keys (`R = r*G`) enable one-time key recovery by the recipient only.

## Known Limitations

This is testnet software. The following are known limitations:

- **Ring members** are selected randomly, not grouped by matching commitment (amount privacy is partial — a determined adversary with chain analysis may infer amounts)
- **Coinbase outputs** use public ed25519 keys, not stealth addresses (miner reward is visible)
- **Single executable** — no separate wallet daemon, no hardware wallet support
- **No DDoS protection** on the public API endpoint
- **Testnet DAG** is 4 MB (mainnet target: ~40 GB)

## Threat Model

**Adversary capabilities assumed:**
- Passive observer of the blockchain (all public data)
- Active network participant (can submit transactions, run nodes)
- Can deploy up to 49% of mining bandwidth

**Adversary capabilities NOT assumed:**
- Breakage of elliptic curve discrete log (Curve25519)
- Breakage of SHA3/Keccak256
- Breakage of Shake256 (Fiat-Shamir transform)
- Control of >50% of mining bandwidth simultaneously
- Physical access to wallet device

**In-scope attacks:**
- Double-spend via chain reorganization
- Privacy compromise via ring signature analysis
- Forged transactions via signature malleability
- Sybil attacks on P2P network

**Out-of-scope (for testnet):**
- 51% attacks (no economic penalty yet)
- Long-range attacks (no checkpoints)
- Side-channel attacks on wallet implementations

## Whitepaper & Specs

- [Whitepaper v27](docs/whitepaper-v27.md)
- [Spec v7](docs/spec-v7.md)
- [Mining Guide](MINING.md)
- [App Architecture](App/ARCHITECTURE.md) (Desktop Miner + Mobile Wallet)
- [Project Scope](App/SCOPE.md)

## Security

Report vulnerabilities to the security contacts listed in [SECURITY.md](SECURITY.md).

## License

MIT — see [LICENSE](LICENSE).

---

*Note: "4Ewatts" is the GitHub organization handle. The protocol and product are named "eWatts".*
