# eWatts Mainnet — Genesis Parameters

*Manual update for protocol v0.5 — June 2026*

## Core Protocol

| Parameter | Value | Source |
|-----------|-------|--------|
| Protocol version | `0x0005` (v0.5, AOPS commitment) | `constants::PROTOCOL_VERSION` |
| Target block time | 600 s | `TARGET_BLOCK_TIME_SECS` |
| Blocks per day | 144 | `BLOCKS_PER_DAY` |
| Blocks per year | 52,596 | `BLOCKS_PER_YEAR` |
| VR window | 1,000 blocks | `VR_WINDOW_BLOCKS` |
| Decimal places | 6 | `DECIMAL_PLACES` |
| Units per Ewatt | 1,000,000 | `UNITS_PER_EWATT` |

## DAG (Proof-of-Work)

| Parameter | Value | Source |
|-----------|-------|--------|
| Initial size | 8 GB | `DAG_INITIAL_SIZE_BYTES` |
| Growth rate | 512 MB/year | `DAG_GROWTH_RATE_BYTES_PER_YEAR` |
| Epoch size | 2,016 blocks (~2 weeks) | `DAG_EPOCH_BLOCKS` |
| Mix rounds | 256 | `DAG_MIX_ROUNDS` |
| Element size | 64 bytes | `DAG_ELEMENT_SIZE` |
| Acceleration rate | 1 GB | `DAG_ACCELERATION_RATE` |
| Acceleration threshold | ETA < 1.3x target | `DAG_ACCELERATION_THRESHOLD_ETA` |
| Acceleration bandwidth | < 100 GB/s | `DAG_ACCELERATION_THRESHOLD_BANDWIDTH` |
| Base accesses per attempt | 1,000,000,000 | `BASE_ACCESSES` |
| Verification sample rate | 0.001 (0.1%) | `VERIFICATION_SAMPLE_RATE` |

## Emission (v0.5)

| Parameter | Value | Source |
|-----------|-------|--------|
| Base emission | 100 Ewatt/block | `BASE_EMISSION` |
| Emission floor | 5 Ewatt/block (0.05x) | `EMISSION_FLOOR_MULTIPLIER` |
| Emission ceiling | 2,000 Ewatt/block (20x) | `EMISSION_CEILING_MULTIPLIER` |
| Ramp-up blocks | 10,000 (~70 days) | `RAMP_UP_BLOCKS` |
| Ramp-up cap | 80% per miner | `RAMP_UP_CAP` |
| Founder lock | 50,000 blocks (~347 days) | `FOUNDER_LOCK_BLOCKS` |
| Founder lock additional | 40,000 blocks | `FOUNDER_LOCK_ADDITIONAL` |

## Commitment System

| Parameter | Value | Source |
|-----------|-------|--------|
| Min AOPS | 20,000,000 ops/s | `MIN_COMMIT_AOPS` |
| Commit window | 4,300 blocks (~30 days) | `COMMIT_WINDOW_BLOCKS` |
| Efficiency penalty | n < 0.7 | `EFFICIENCY_PENALTY_THRESHOLD` |
| Efficiency cap | n > 1.3 | `EFFICIENCY_CAP_THRESHOLD` |

## Energy Model

| Parameter | Value | Source |
|-----------|-------|--------|
| J_PER_ACCESS | 3.75 µJ | `J_PER_ACCESS` (75W / 20M ops) |
| Node power | 75 W | `WATTS_PER_NODE` |
| J/kWh | 3,600,000 | `J_PER_KWH` |
| DDR5 calibration | 3.75 µJ/access | `J_PER_ACCESS_DDR5` |
| DDR4 calibration | 5.0 µJ/access | `J_PER_ACCESS_DDR4` |
| DDR3 calibration | 10.0 µJ/access | `J_PER_ACCESS_DDR3` |

## Privacy

| Parameter | Value | Source |
|-----------|-------|--------|
| Ring signature size | 11 | `RING_SIGNATURE_SIZE` |
| Quantum activation block | 3,153,600 (~6 years) | `QUANTUM_ACTIVATION_BLOCK` |
| PQ signature scheme | FALCON-1024 | `PQ_SIG_SCHEME` |

## P2P

| Parameter | Value | Source |
|-----------|-------|--------|
| Max peers | 125 | `MAX_PEERS` |

## Genesis Supply

| Parameter | Value |
|-----------|-------|
| Total supply | 1,000,000 Ewatt (1M) |
| Distribution | Founder allocation, time-locked |
| Genesis key | Deterministic, published before block 1 |

---

*Note: The auto-generation script from v28 has been deprecated. Values above reflect protocol v0.5 codebase as of June 2026.*
