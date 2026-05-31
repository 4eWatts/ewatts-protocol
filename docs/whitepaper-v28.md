# Ewatts Protocol v28 — Neutral Settlement Layer

**DRAM-Bound Proof-of-Energy — Whitepaper**
*June 2026*

**Ewatts is not a store of value. It is a ruler.**

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| v17-v22 | Pre-May 2026 | Original single-chain, single-hash MBPoW |
| v23 | 16 May 2026 | Bandwidth commitment model, VR, bootstrap mechanics |
| v24 | 17 May 2026 | Dual-chain (L1 + L2 bridge) — DEPRECATED |
| v25 | 17 May 2026 | Single-chain, dual-hash |
| v26 | 17 May 2026 | Privacy by default, selective disclosure, J_GB risk, L2 future, quantum |
| v27 | 17 May 2026 | R_min=0.05x, R_max=20x, Historical_Avg=30d, RampUpFactor 80% cap, coinbase_burn. Founder time-locks. P2PKH addresses. |
| **v28** | **June 2026** | **Bootstrap multiplier M(S) = M_MAX x exp(-k x S/S_threshold). Lookup table for deterministic cross-platform computation. Cost-anchored emission: E = total_eff x M(S) x C_node / denominator. v27 formula fully deprecated.** |

---

## 1. Core Thesis

Ewatts is a neutral settlement layer anchored to physical energy cost. It is not a store of value, a speculative asset, or a medium of exchange in the traditional sense. It is a ruler: a fixed, transparent unit that tracks the dollar-denominated cost of provable computation.

The protocol achieves this by making the following equivalent:

- 1 GB/s of provable DRAM bandwidth
- ~6 J of energy consumed per second
- A known, measurable dollar cost ($0.0020625/block/node at calibration)

The result is a cryptocurrency whose emission rate is proportional to the real energy committed to the network, with a bootstrap multiplier that decays exponentially as the network matures.

---

## 2. Emission Formula (v28 Final)

The v28 formula replaces the v27 dual-mode formula entirely. Emission is now a function of:

- `total_eff`: total effective commitment of all miners (in COMMIT_PRECISION units)
- `M(S)`: bootstrap multiplier, a function of total supply
- `C_node`: cost per node per block ($0.0020625, amplified to integer arithmetic)

### 2.1 Bootstrap Multiplier M(S)

```
M(S) = M_MAX x exp(-k x S / S_threshold)

Where:
  M_MAX = 100,000          (bootstrap multiplier at genesis)
  k = ln(M_MAX) ≈ 11.5129 (decay constant)
  S_threshold = 10^16     (10 billion Ewatt in base units)
```

At genesis (S=0), M(S) = 100,000. At maturity (S >= S_threshold), M(S) = 1.0. The multiplier decays exponentially, giving early miners 100,000x more emission per unit of committed bandwidth than mature miners.

The function is pre-computed into a 4096-entry lookup table (32 KB) at first use via `OnceLock`. Linear interpolation between table entries ensures bit-exact determinism across all platforms.

### 2.2 Cost-Anchored Emission Rate

```
E_block = total_eff x M_prec x C_node_amplified / 1e18

Where:
  total_eff       total effective commitment (COMMIT_PRECISION = 1e9)
  M_prec          bootstrap multiplier (EMISSION_PRECISION = 1e9)
  C_node_amplified = 2,062,500 (0.0020625 USD x 1e9)
  1e18            precision normalization
```

At calibration (100,000 miners at 1 GB/s each, network mature):

```
total_eff = 100,000 x 1e9 = 1e14
M_prec = 1 x 1e9 = 1e9
E_block = 1e14 x 1e9 x 2,062,500 / 1e18 = 206.25 eWatt/block
```

At genesis (single miner at 1 GB/s, S=0):

```
total_eff = 1e9
M_prec = 100,000 x 1e9 = 1e14
E_block = 1e9 x 1e14 x 2,062,500 / 1e18 ≈ 20,625 eWatt/block
```

The bootstrap multiplier gives the early network 100,000x more emission. This is not inflation; it is mechanical. At 10-minute blocks, a solo genesis miner would emit ~206k eWatt/day, declining exponentially toward the maturity rate of ~2,074 eWatt/day (for 10k calibrated nodes).

### 2.3 Parameters

| Parameter | Value | Notes |
|-----------|-------|-------|
| M_MAX | 100,000 | Bootstrap multiplier at block 1 |
| S_threshold | 10 billion Ewatt | Maturity threshold |
| C_node | $0.0020625/block | Cost of 1 GB/s for 600s at $0.12/kWh |
| N_calibration | 100,000 | Reference network size |
| P_target | $1.00/eWatt | Target production cost at maturity |
| Block time | 600s (10 min) | Target |
| J_PER_GB | 6.0 J | Total node energy per GB of memory access |
| Ramp-up | 10,000 blocks (~70 days) | 80% cap on single miner share |
| Founder lock | 50,000 blocks (~347 days) | Lock-until-min(50000, block+40000) |

### 2.4 Distribution

- Initial supply: 1,000,000 Ewatt (mainnet) or 1,000,000 Ewatt (testnet)
- All initial supply goes to a single genesis address
- No pre-mine, no ICO, no foundation allocation beyond the genesis key
- Emission is 100% miner rewards, starting from block 1

### 2.5 Founder Mining

Founder mining outputs are locked until:
- max(50,000, block_number + 40,000) blocks during ramp-up
- No lock after ramp-up (block >= 10,000)

During ramp-up (first ~70 days), any single miner is capped at 80% of the block reward. The remaining 20% is burned. This creates a mechanical incentive to invite other miners.

---

## 3. Self-Balancing — Emergent Consequence

The v28 formula self-balances through two mechanisms:

**1. Bootstrap multiplier decay.** As supply grows, M(S) shrinks. Early miners capture more reward per GB/s, compensating for sparse network effects. Late miners receive less, but benefit from a mature, stable network.

**2. Cost anchoring.** The emission rate is proportional to total committed bandwidth. If the network grows, emission rises proportionally. If it shrinks, emission falls. The ratio of emission to committed work is fixed at calibration, modulo the bootstrap multiplier.

This is not a peg, a target, or a monetary policy. It is a physical constant expressed in protocol terms.

---

## 4. Dual-Hash Architecture

### 4.1 Mining Hash (MBPoW, 600s)

- Uses a 64-element mix of FNV-1a double-hashed DAG entries
- DAG starts at 8 GB (mainnet) or 4 MB (testnet)
- Grows 512 MB/year, with acceleration if block time drops below 1.3x target
- Verification: random sample of 0.1% of accesses (~1M/block verified)
- Output: 32-byte proof hash included in each block

### 4.2 Transaction Hash (Fast, <3ms)

- Blake3-based transaction inclusion proof
- Does not require the full DAG
- Used for mempool, wallet, and light client verification

---

## 5. Privacy

### 5.1 Intocável (Untouchable)

All transactions use MLSAG ring signatures (ring size 11) by default. There is no opt-out, no plain-text mode, no "compliant" override. Ring membership is drawn from recent UTXOs across all miners, not just the sender.

The protocol does not know who is transacting. It only knows that the transaction was signed by one of the ring members.

### 5.2 Institutional Capture Protection

A 95% miner/node supermajority vote is required for any protocol upgrade that would reduce privacy, increase supply caps, or change the emission formula. This threshold is hard-coded in the genesis block and can only be changed by a hard fork with the same threshold.

### 5.3 P2PKH-Style Addresses

Despite ring signatures, addresses look like Bitcoin-style P2PKH (1-byte version + 20-byte pubkey hash). This simplifies exchange integration and wallet UX while the underlying privacy is preserved.

---

## 6. J_GB — Protocol Parameter

### 6.1 Fixed at Genesis

J_GB (joules per gigabyte of memory access) is the conversion factor from DRAM bandwidth to energy. For v28:

```
J_GB = 6.0 J/GB
```

This includes CPU, DRAM refresh, chipset idle, and PSU overhead for a reference mining node (~75W at 12.5 GB/s effective bandwidth).

### 6.2 Recalibration via Hard Fork

If empirical measurement from wattmeter data shows significant deviation (more than 30%), a hard fork may recalibrate J_GB. The same 95% miner supermajority is required.

### 6.3 Equipment

Any DRAM-equipped machine can mine. There is no ASIC path, no FPGA advantage, no GPU dependency. The bottleneck is memory bandwidth, which is commodity and anti-fragile.

---

## 7. Future: Layer 2

L2 development is post-launch. The protocol exposes:

- UTXO-based state for easy sidechain/rollup bridging
- Privacy-preserving commitments as a base layer for confidential L2
- The dual-hash architecture allows L2 to use the fast hash for checkpointing

No L2 is specified in v28. The interface is documented in the spec.

---

## 8. Known Risks

| Risk | Mitigation |
|------|------------|
| J_GB mis-calibration | Empirical measurement, hard fork path |
| Low miner count at launch | Bootstrap multiplier M_MAX=100k compensates |
| Exchange listing delay | P2PKH addresses, standard UTXO model |
| Quantum computing | FALCON-1024 migration path at block 3,153,600 |
| DRAM supply concentration | DRAM is global commodity, 3 manufacturers competing |
| 51% attack | DAG memory requirement makes rental impractical |

---

*Ewatts Protocol v28 — June 2026*
