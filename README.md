# Ewatts Protocol

**Memory-Bound Digital Currency — DRAM-Bound Proof-of-Energy**

This is the reference implementation of the Ewatts protocol, a neutral digital currency whose issuance is constrained by verifiable DRAM bandwidth competition.

## Architecture

```
src/
├── main.rs          Entry point
├── constants.rs     Protocol constants (spec §2)
├── dag.rs           DAG generation (spec §4)
├── proof.rs         MBPoW mining + verification (spec §5)
├── commitment.rs    Bandwidth commitment system (spec §3)
├── vr.rs            VR — Valor de Referência (spec §11)
├── block.rs         Block structure and validation (spec §9)
├── reward.rs        Reward calculation + emission (spec §6-7)
└── difficulty.rs    Difficulty adjustment (spec §8)
```

## Quick Start

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Mining (Testnet)

```rust
use ewatts_protocol::{dag::Dag, proof::mine};
let dag = Dag::generate(0, false);
let header = [0u8; 32];
let solution = mine(&header, 1, &dag, 100_000).unwrap();
```

## Specification

Detailed specifications:
- [Whitepaper v23](https://github.com/4ewatts/ewatts-protocol)
- [Spec v3](https://github.com/4ewatts/ewatts-protocol)

## License

MIT
