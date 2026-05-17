# Ewatts Protocol v7 — Formal Specification (Final)

**Single-Chain, Dual-Hash, Privacy-by-Default Memory-Bound Digital Currency**
*May 2026*

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| v1-v3 | Pre-May 2026 | Single-hash MBPoW |
| v4 | 17 May 2026 | Dual-chain — DEPRECATED |
| v5 | 17 May 2026 | Dual-hash |
| v6 | 17 May 2026 | Privacy, selective disclosure, quantum |
| **v7** | **17 May 2026** | **Formula final com RampUpFactor. P2PKH. Captura institucional. J_GB recalibração via hard fork. Selective disclosure removido.** |

---

## 1. Emission Formula

```
R(block) = BASE_EMISSION × (Total_Effective_Commitment / Historical_Avg_Commitment)
           clamped to [BASE × 0.05, BASE × 20]
           × RampUpFactor(block) se block < 10.000

RampUpFactor:
  se Reward_i / R(block) > 0.80:
    Reward_i = 0.80 × R(block)
    excess → coinbase_burn

Founder time-locks:
  cada coinbase output mined < bloco 10.000:
    spendable_after = max(50000, current_block + 40000)
```

### Constants

| Constant | Value |
|----------|-------|
| BASE_EMISSION | 100 Ewatt/bloco |
| HISTORICAL_AVG_WINDOW | 4.300 blocos (30 dias) |
| R_MIN | 5 Ewatt/bloco (0,05×) |
| R_MAX | 2.000 Ewatt/bloco (20×) |
| RAMP_UP_BLOCKS | 10.000 |
| RAMP_UP_CAP | 80% |
| FOUNDER_LOCK_BLOCKS | max(50000, current + 40000) |

---

## 2. Mining Hash

MBPoW unchanged from v3: bandwidth commitments, DAG walk, efficiency η, VR.

---

## 3. Transaction Hash

Ring signature verification (<3ms), stealth address, confidential amounts. No disclosure mechanism.

---

## 4. Privacy — Immutable

Section 5.2 of v27 applies: any privacy change requires hard fork with 95% miners + 95% nodes.

### 4.1 P2PKH Addresses

```rust
struct UtxoOutput {
    amount: PedersenCommitment,
    pubkey_hash: [u8; 20],      // H(public_key) instead of public key
    range_proof: RangeProof,
}
// Public key revealed only when spending
```

---

## 5. J_GB

| Parameter | Value |
|-----------|-------|
| J_GB (genesis) | 0,08 J/GB |
| Calibration | DDR5 |
| Recalibration | Hard fork periódico (2-3 anos), não auto-ajuste |

---

## 6. Quantum Migration

### 6.1 P2PKH Baseline (Genesis)

All addresses use pubkey_hash, not direct pubkey. Protects against retroactive quantum theft.

### 6.2 Activation Parameters

```rust
const QUANTUM_ACTIVATION_BLOCK: u64 = 3_153_600;  // ~10 anos
const PQ_MINER_SUPERMAJORITY: f64 = 0.90;
const PQ_SIG_SCHEME: &str = "FALCON";               // NIST selected
```

---

## 7. Constants Summary

| Constant | Value | Scope |
|----------|-------|-------|
| TARGET_BLOCK_TIME | 600 s | Genesis |
| BASE_EMISSION | 100 Ewatt/bloco | Genesis |
| HISTORICAL_AVG_WINDOW | 4.300 blocos (30 dias) | Genesis |
| R_MIN | 0,05× (5 Ewatt) | Genesis |
| R_MAX | 20× (2.000 Ewatt) | Genesis |
| RAMP_UP_BLOCKS | 10.000 | Genesis |
| RAMP_UP_CAP | 80% | Genesis (removido após 10.000) |
| FOUNDER_LOCK | max(50000, current+40000) | Genesis |
| DAG_INITIAL_SIZE | 8 GB | Genesis |
| J_GB | 0,08 J/GB | DDR5 calibration |
| VR_WINDOW | 1.000 blocos | Genesis |
| MIXIN_COUNT | 11 (padrão) | Privacy |
| TX_SIZE | ~2,8 KB | Ring sig + stealth + CA |
| QUANTUM_ACTIVATION | 3.153.600 | Genesis |
| PQ_SIG | FALCON (666 B) | NIST selected |
| PRIVACY_CHANGE_CONSENSUS | 95% miners + 95% nodes | Hard fork required |

---

*Ewatts Protocol v7 — May 2026*
