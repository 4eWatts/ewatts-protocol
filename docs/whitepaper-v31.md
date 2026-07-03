# Ewatts Protocol v31 — Neutral Settlement Layer

**DRAM-Bound Proof-of-Energy — Whitepaper**
*July 2026*

**Ewatts is not a store of value. It is a ruler.**

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| v17-v22 | Pre-May 2026 | Original single-chain, single-hash MBPoW |
| v23 | 16 May 2026 | Bandwidth commitment model, VR, bootstrap mechanics |
| v24-v27 | 17 May 2026 | Single-chain, dual-hash, privacy, ramp-up, founder locks |
| v28 | June 2026 | Bootstrap multiplier proposal (not implemented) |
| **v29** | **June 2026** | **AOPS-based commitment replaces GB/s. Real emission formula from code. J_PER_ACCESS wall-power calibration. DRAM-energy correlation explicit. All numbers reconciled with protocol v0.5.** |
| **v30** | **June 2026** | **Precise DRAM latency data with tRC values. Academic references added. Bandwidth vs. latency distinction clarified.** |
| **v31** | **July 2026** | **Free-entry equilibrium framework. VR = P/p_elec. Efficiency drift absorbed by network size. Burn auction shown unnecessary. Real risk is bootstrap entry lag.** |

---

## 1. Core Thesis

Ewatts is a neutral settlement layer anchored to the physical cost of electricity. It is not a store of value, a speculative asset, or a medium of exchange in the traditional sense. It is a ruler: a fixed, transparent unit whose production cost is determined by verifiable energy expenditure.

The protocol achieves this by making the following equivalent:

- 20 million random DRAM accesses per second (AOPS)
- ~75W of wall power per mining node
- A measurable, auditable energy cost per eWatt minted

The result is a cryptocurrency whose emission rate is proportional to the real access operations committed to the network, with an elastic supply that self-balances via market arbitrage against production cost.

---

## 2. Protocol Constants

### 2.1 Core

| Parameter | Value | Notes |
|-----------|-------|-------|
| PROTOCOL_VERSION | 0x0005 | v0.5 — AOPS commitment |
| Block time | 600s (10 min) | TARGET_BLOCK_TIME_SECS |
| Blocks per year | 52,596 | BLOCKS_PER_YEAR |
| Base emission | 100 Ewatt/block | BASE_EMISSION |
| Emission floor | 5 Ewatt/block (0.05x) | EMISSION_FLOOR_MULTIPLIER |
| Emission ceiling | 2,000 Ewatt/block (20x) | EMISSION_CEILING_MULTIPLIER |
| Decimal places | 6 | 1 Ewatt = 1,000,000 base units |

### 2.2 Commitment

| Parameter | Value | Notes |
|-----------|-------|-------|
| Min AOPS | 20,000,000 ops/s | 20M random accesses — DDR baseline |
| Min commit (rolling) | max(20M, 0.1 × median) | Adjusts to network |
| Commit window | 4,300 blocks (~30 days) | COMMIT_WINDOW_BLOCKS |
| Efficiency penalty | <0.7 | Effective = declared × efficiency |
| Efficiency cap | >1.3 | Effective = declared × 1.3 |
| Element size | 64 bytes | DAG_ELEMENT_SIZE |

### 2.3 Energy

| Parameter | Value | Notes |
|-----------|-------|-------|
| J_PER_ACCESS | 3.75 µJ | 75W / 20M ops — wall power per random access |
| J_PER_KWH | 3,600,000 | Conversion |
| DDR3 calibration | ~10 µJ/access | Legacy hardware (10W/1M ops) |
| DDR4 calibration | ~5 µJ/access | Mid-range |
| DDR5 calibration | ~3.75 µJ/access | Modern, baseline |
| Node power | 75W | Reference mining node (DRAM + CPU + PSU) |

### 2.4 Emission

| Parameter | Value | Notes |
|-----------|-------|-------|
| Ramp-up blocks | 10,000 (~70 days) | 80% cap on single miner share |
| Founder lock | 50,000 blocks (~347 days) | Lock-until-min(50000, block+40000) |
| Initial supply | 1,000,000 Ewatt | Single genesis address |

### 2.5 Privacy

| Parameter | Value | Notes |
|-----------|-------|-------|
| Ring signature size | 11 | MLSAG, always-on |
| Governance threshold | 95% | Miner/node supermajority |

---

## 3. Emission Formula

The emission formula is straightforward and proportional to committed work. This is what the protocol actually implements; there is no bootstrap multiplier, no cost-of-production denominator, and no synthetic price target.

### 3.1 Rate

```
E_block = BASE_EMISSION × total_effective_aops / historical_avg_aops

Where:
  BASE_EMISSION        = 100 Ewatt/block (hard-coded)
  total_effective_aops = sum of all miners' effective AOPS commitments
  historical_avg_aops  = rolling average over VR window (1,000 blocks)
```

Result is clamped:
- **Floor**: 5 Ewatt/block (0.05x base)
- **Ceiling**: 2,000 Ewatt/block (20x base)

### 3.2 Examples

**Stable network** (total_eff == historical_avg):
```
E_block = 100 × 1.0 = 100 Ewatt/block
~14,400 Ewatt/day at 600s blocks
```

**Doubled hashrate** (total_eff == 2× historical_avg):
```
E_block = 100 × 2.0 = 200 Ewatt/block
```

**Empty network** (total_eff → 0): floor applies at 5 Ewatt/block

**Solo miner** (25M AOPS = 1 node, historical_avg was 25M):
```
E_block = 100 × 25,000,000 / 25,000,000 = 100 Ewatt/block (single miner)
```

### 3.3 Reward Distribution

Each miner receives proportionally to their effective AOPS:

```
reward_i = (c_eff_i / total_eff) × E_block
```

Where `c_eff_i` is the miner's effective commitment after efficiency adjustment:

```
efficiency = total_ops_delivered / (declared_ops_per_sec × time)
c_eff = declared_ops_per_sec × clamp(efficiency, 0.7, 1.3)
```

- efficiency < 0.7: penalized (c_eff = declared × efficiency)
- efficiency > 1.3: capped (c_eff = declared × 1.3)
- efficiency between 0.7 and 1.3: c_eff = declared (no adjustment)

### 3.4 Ramp-Up Cap

During the first 10,000 blocks (~70 days), no single miner receives more than 80% of the block reward. The excess is burned (sent to coinbase without an owner). This creates a mechanical incentive to invite other miners.

### 3.5 Founder Lock

Outputs mined before block 10,000 are locked until:
```
max(50,000, block_mined + 40,000) blocks
```

After block 10,000, no lock applies.

---

## 4. The DRAM-Energy Correlation

The central insight of eWatts is that DRAM random access latency is a physical constant that maps directly to energy consumption.

### 4.1 How a Random Access Consumes Energy

Each random DRAM access involves:

1. **Memory controller** — decodes the address and selects the rank/bank
2. **Row activation** — RAS strobe opens the row, sense amps amplify the bitline charge
3. **Column read** — CAS strobe selects the column, data is driven onto the bus
4. **Prefetch + transfer** — DDR prefetches multiple bits, transfers on both clock edges
5. **Output buffers** — drive the data across the memory bus to the CPU

All five steps consume energy. In a random access pattern, the row is almost never already open (unlike sequential access), so every access pays the full row activation cost. This is why random access latency is 10-20x higher than sequential bandwidth would suggest, and why it consumes consistently more energy per byte.

### 4.2 Wall Power vs Datasheet Energy

Datasheet J_PER_ACCESS values (3-5 µJ for DDR4, 2-3 µJ for DDR5) only account for the DRAM chip. A real mining node includes:

- CPU memory controller (~15-20W)
- DRAM modules at load (~15-25W per channel)
- Chipset and bus (~5-10W)
- PSU losses (~10-15%)

The protocol uses **J_PER_ACCESS = 3.75 µJ**, derived from a 75W reference node at 20M random accesses per second. This is wall power, not chip-only data. It intentionally overestimates chip-level energy to account for the full system.

### 4.3 Stability Across Generations

| Generation | Max BW | Random Access Latency (tRC) | J_PER_ACCESS (wall) | Relative Efficiency |
|-----------|--------|-----------------------------|--------------------|--------------------|
| DDR1 | 3.2 GB/s | ~52-60 ns | ~22 µJ | — |
| DDR2 | 6.4 GB/s | ~50-54 ns | ~15 µJ | — |
| DDR3 | 12.8 GB/s | ~46-50 ns | ~10 µJ | 1.0x (baseline) |
| DDR4 | 25.6 GB/s | ~44-48 ns | ~5 µJ | 2.0x |
| DDR5 | 48 GB/s | ~48-52 ns | ~3.75 µJ | 2.7x |

**Latency (tRC) has remained essentially flat at ~45-60 ns for over 25 years** [1][2][3], improving only ~1.3x across DDR1 through DDR5 — an annual compound rate of approximately 1%. [4][5] This is the memory wall: a physical limit set by bitline capacitance, sense amplifier settling time, and wire RC delay. These parameters do not scale with process shrinks the way transistor density does.

**Energy per access (J_PER_ACCESS) has improved more significantly** — roughly 2.7x from DDR3 to DDR5, or ~7% CAGR — driven by reduced operating voltage, smaller process nodes, and improved sense amplifier design. This improvement matters for the VR energy calculation but does not affect mining difficulty or the latency-bound nature of the Proof-of-Work.

**The key insight for the protocol:** mining is a latency-bound operation (the DAG walk cannot proceed faster than random access latency allows), not an energy-bound one. Even as J_PER_ACCESS improves with each generation, the fundamental speed of random access remains nearly constant. A miner on DDR5 cannot complete significantly more AOPS per second than a miner on DDR4 — the row activation time has not changed. The energy efficiency improvement is captured by the VR calculation but does not confer a speed advantage in the Proof-of-Work.

**Note on bandwidth vs. latency:** The 10-15x improvement in peak bandwidth between DDR1 and DDR5 reflects wider prefetch (2n to 16n), longer burst lengths, and faster I/O clocks — not faster random cell access. For Memory-Bound Proof-of-Work, what matters is random access latency and its physical constraint on operations per second, not sequential throughput.

The protocol documents calibration values for each generation but uses DDR5 as the baseline. A hard fork via 95% supermajority can recalibrate if empirical data shows systematic deviation.

---

## 5. Free Entry as Implicit Auction

eWatts has no fixed supply cap and no scheduled halvings. Supply is elastic and self-balancing through a single feedback loop: market arbitrage against production cost.

The protocol does not need an explicit bidding mechanism (burn auction, proof-of-burn, or similar). Every node running eWatts is already a continuous bid in an implicit auction. Each node consumes a fixed amount of energy per block. Under free entry and exit, the network finds its own equilibrium size.

### 5.1 Per-Node Cost

Each mining node consumes W watts. Over one 600-second block, the electricity cost is:

```
c = W x 600 x p_elec / 3,600,000    ($/block/node)
```

For a reference DDR5 node (W = 75W, p_elec = $0.10/kWh):

```
c = 75 x 600 x 0.10 / 3,600,000 = $0.00125/block/node
```

### 5.2 Free-Entry Equilibrium

The block reward is fixed at 100 eWatts. With N identical nodes and market price P, total node cost must equal total block reward value under competitive free entry:

```
N x c = 100 x P
```

Substituting the per-node cost:

```
N x (W x 600 x p_elec / 3,600,000) = 100 x P
N = 8,000 x P / p_elec    (for W = 75W)
```

**The equilibrium node count N is proportional to P / p_elec.** Node power W cancels when fixed to the reference value. A doubling of eWatt price doubles the node count. A doubling of electricity cost halves it.

### 5.3 Network Energy and VR in Equilibrium

Network energy per block at equilibrium:

```
E = N x W x 600 / 3,600,000
E = 100 x P / p_elec
```

The watts-per-node cancel completely. Total network energy depends only on market price and electricity cost. The Value Reference at equilibrium is:

```
VR = E / 100 = P / p_elec
```

**VR converges to P / p_elec. Hardware efficiency does not appear in the equation.**

### 5.4 What Efficiency Drift Actually Does

When hardware efficiency improves (DDR5 replacing DDR4, better process nodes, lower voltage), W per node drops while maintaining the same ~1 GB/s bandwidth. The sequence is:

1. c drops (cost per node falls)
2. Margin opens (revenue exceeds cost)
3. New nodes enter, N rises
4. At equilibrium: N x c returns to 100 x P
5. Total network energy E stays constant
6. VR stays at P / p_elec

A 5% efficiency improvement -> 5% more nodes. The anchor is unchanged. The drift is absorbed entirely by network size, not by the Value Reference.

**The only scenario where efficiency drift changes the VR:** the scenario where N is frozen, that is where the free-entry mechanism is disabled by construction. Earlier whitepaper simulations (v28-v30) assumed frozen N and showed 5-7%/year drift. Those simulations modeled a market without the market mechanism.

### 5.5 Comparison to Bitcoin

Bitcoin has the same fundamental identity. The Cambridge Centre for Alternative Finance estimates Bitcoin's energy consumption using exactly this production-cost-equals-revenue identity. On Bitcoin:

- Mining revenue = block reward + fees
- Mining cost = hardware + electricity
- Under competition: cost approximates revenue
- Energy tracks price

The structural difference is in adjustment speed:

- **Bitcoin**: ASIC manufacturing takes months. Lead time means a profitability signal takes 3-6 months to translate into new hash rate. Old ASICs become obsolete when new generations ship. The adjustment has a lag of quarters.
- **eWatts**: The mining node is commodity DRAM, billions of devices already have it. Any computer can run the node. Entry and exit take minutes. The adjustment is near-instantaneous.

eWatts free entry is closer to the theoretical ideal than any existing Proof-of-Work network.

### 5.6 Why a Burn Auction Is Unnecessary

An explicit bidding mechanism (miners burning eWatt for additional reward allocation) would also stabilize the system. But compare where the margin goes in each design:

| Mechanism | Margin absorbed by | Effect |
|-----------|-------------------|--------|
| Free entry | New nodes | More distribution, wider decentralization, hardware anyone can supply |
| Burn auction | Supply destruction | Same incumbents remain dominant. Creates a barrier: need eWatt to bid (injects a proof-of-stake component into consensus) |

For a protocol whose pitch is accessibility and neutrality, absorbing margin via entry is strictly superior. The burn auction solves a problem that free entry already solves and it pays for that solution with the protocol's main differentiator.

### 5.7 The Real Risk

The equilibrium assumes instant entry and exit. In practice, during the bootstrap phase with few nodes and low liquidity, the response to a margin signal has delay. During that delay, efficiency drift affects real returns.

- This risk is real, measurable, and temporary
- It exists on every young PoW network
- eWatts minimizes it relative to other PoW (no ASIC fabrication lag)
- The testnet must demonstrate: does entry respond to margin?

**The only hypothesis the testnet needs to validate:** elasticity of entry. If a margin of 10% attracts 10% more nodes within a predictable window, the equilibrium holds. If nodes do not respond, the drift scenario is real.

### 5.8 Example Scenarios

| Scenario | P | p_elec | N | E (kWh/block) | VR |
|----------|---|--------|----|-------------|-----|
| Bear | $0.10 | $0.10 | 8,000 | 100 | 1 kWh/eWatt |
| Base | $1.00 | $0.10 | 80,000 | 1,000 | 10 kWh/eWatt |
| Bull | $5.00 | $0.10 | 400,000 | 5,000 | 50 kWh/eWatt |
| Expensive power | $1.00 | $0.30 | 26,667 | 333 | 3.33 kWh/eWatt |

## 6. VR — Value Reference

VR (Valor de Referência) is the protocol's internal meter: it converts committed access operations into a kilowatt-hour per eWatt ratio.

### 6.1 Formula

```
total_secs = window_blocks × block_time
total_accesses = avg_effective_aops × total_secs
total_joules = total_accesses × J_PER_ACCESS
total_kwh = total_joules / J_PER_KWH
VR = total_kwh / total_ewatts_mined
```

### 6.2 Interpretation

- VR = 0.01 kWh/Ewatt means the network spent 0.01 kWh of real electricity to mint one eWatt during the window
- VR is purely computational — it derives energy from the access operations declared and verified, not from external oracles
- VR is a lagging indicator (1,000-block window) and cannot be manipulated within a single block
- Multiply VR by local electricity price to get the dollar-denominated production cost floor

### 6.3 Example

A solo miner running 25M AOPS for 1,000 blocks (600,000 seconds):

```
total_accesses = 25,000,000 × 600,000 = 1.5 × 10^13
total_joules = 1.5 × 10^13 × 3.75 × 10^(-6) = 56,250,000 J
total_kwh = 56,250,000 / 3,600,000 = 15.625 kWh
```

If the miner earned 100 Ewatt/block × 1,000 blocks = 100,000 Ewatt:

```
VR = 15.625 / 100,000 = 0.000156 kWh/Ewatt = 156 Wh/Ewatt
```

At $0.12/kWh: production cost floor ≈ $0.0187 per eWatt.

### 6.4 VR in Equilibrium

Under the free-entry equilibrium derived in Section 5, the VR converges to a simple ratio of market price to electricity cost:

```
VR = P / p_elec
```

This result follows directly from the equilibrium identity N x c = 100 x P:

- c = W x 600 x p_elec / 3,600,000 (per-node cost per block)
- N x c = 100 x P (zero-margin equilibrium)
- E = N x W x 600 / 3,600,000 = 100 x P / p_elec (network energy per block)
- VR = E / 100 = P / p_elec

**Hardware efficiency (watts per node) cancels completely.** A DDR3 node consuming 150W and a DDR5 node consuming 75W arrive at the same VR at equilibrium, because the less efficient node simply earns less per watt while the more efficient node allows more nodes to enter.

Key implications:
- VR is not a fixed number of kWh. It floats with the market price of eWatt and the cost of electricity.
- The value-energy relationship is invariant, not a specific kWh value.
- Efficiency drift does not change the equilibrium VR. It changes the number of nodes needed to maintain it.
- The computational VR from Section 6.1 converges to this equilibrium VR as the network approaches steady state.



## 7. Proven-of-Work (MBPoW)

### 7.1 Mining

- DAG-based memory-hard PoW using SHA512 walking
- 1 billion base accesses per hash attempt (scaled by difficulty)
- Each step: read DAG[pos] (64 bytes), XOR with mix (64 bytes), SHA512 of mix
- DAG starts at 8 GB, grows 512 MB/year
- Epoch: 2,016 blocks (~2 weeks)
- Acceleration: if block time < 1.3x target AND committed bandwidth < 100 GB/s, grow 1 GB extra

### 7.2 Verification

Proof is a trace of randomly sampled intermediate steps (0.1% of accesses). Three verification paths:

1. **Full walk**: verify all N accesses (fallback)
2. **Sampled with Merkle root**: verify 30 random samples against committed Merkle root
3. **Legacy sequential**: backward-compatible sequential check

Each block includes a WorkReport converting the solution into an AOPS-equivalent claim.

### 7.3 Dual Hash

The mining hash (SHA512-heavy, DAG-dependent) is separate from the transaction hash (Blake3, <3ms). Transactions do not require the DAG and are verified independently.

---

## 8. Privacy

### 8.1 Intocável (Untouchable)

All transactions use MLSAG ring signatures (ring size 11) by default. There is no opt-out, no plain-text mode, no compliant override. Ring membership is drawn from recent UTXOs across all miners, not just the sender.

The protocol does not know who is transacting. It only knows that the transaction was signed by one of the ring members.

### 8.2 Cryptographic Components

- **Stealth addresses**: one-time recipient addresses derived from a Diffie-Hellman key exchange, preventing linkability
- **Pedersen commitments**: hide transaction amounts while keeping the total supply auditable
- **Range proofs**: prove amounts are non-negative without revealing the value
- **Key images**: prevent double-spending without revealing which UTXO was spent

### 8.3 P2PKH-Style Addresses

Despite ring signatures, addresses look like Bitcoin-style P2PKH (1-byte version + 20-byte pubkey hash). This simplifies exchange integration and wallet UX while the underlying privacy is preserved.

### 8.4 Institutional Capture Protection

A 95% miner/node supermajority vote is required for any protocol upgrade that would reduce privacy, increase supply caps, or change the emission formula. This threshold is hard-coded in the genesis block and can only be changed by a hard fork with the same threshold.

---

## 9. Commitment System

Miners declare their bandwidth through signed commitments. Each commitment contains:

- `miner_id`: ed25519 public key (32 bytes)
- `access_ops_per_sec`: declared AOPS
- `block_number`: block at time of commitment
- `total_access_ops`: actual access operations delivered
- `time_seconds`: measurement period
- `signature`: ed25519 signature binding the commitment to the miner

**Validation**: signature check, AOPS >= network minimum, efficiency > 0

**Penalty for dishonesty**: if delivered < declared × 0.7, effective commitment is reduced proportionally. If delivered > declared × 1.3, effective commitment is capped at declared × 1.3. There is no collateral, no slashing — the only consequence is reduced reward allocation.

---

## 10. Future: Layer 2

L2 development is post-launch. The protocol exposes:

- UTXO-based state for easy sidechain/rollup bridging
- Privacy-preserving commitments as a base layer for confidential L2
- Dual-hash architecture allows L2 to use the fast hash for checkpointing

No L2 is specified in v29. The interface is documented in the spec.

---

## 11. Known Risks

| Risk | Mitigation |
|------|------------|
| Entry elasticity during bootstrap | The equilibrium assumes instant node entry. At low N (<1,000 nodes), the lag between margin signal and actual entry is the only real risk. Testnet must validate that entry responds to margin within a predictable window. |
| J_PER_ACCESS mis-calibration | Empirical wattmeter measurement, hard fork at 95% |
| Exchange listing delay | P2PKH addresses, standard UTXO model |
| Quantum computing | FALCON-1024 migration path at block 3,153,600 |
| DRAM supply concentration | Global commodity, 3 manufacturers competing |
| 51% attack | DAG memory requirement (8 GB) makes rental impractical |
| VR energy derivation | Not an oracle. Purely computational from verified accesses. Under equilibrium, VR = P/p_elec. |

---

## 12. Legacy Notes

### 12.1 J_PER_GB

Previous whitepaper versions referred to J_PER_GB = 0.08 J/GB, calibrated for DDR5 at the DRAM chip level. This value is a derivation from the access-based model:

```
J_PER_GB = J_PER_ACCESS × (1 GB / DAG_ELEMENT_SIZE)
         = 3.75 × 10^(-6) × (10^9 / 64)
         = 58.59 J/GB (wall power)
```

The chip-level value of 0.08 J/GB does not account for system overhead. For accurate energy accounting, prefer the AOPS-based model with J_PER_ACCESS = 3.75 µJ.

### 12.2 Previous Formula Revisions

v27 introduced a dual-mode formula with a 30-day historical average and ramp-up factor. v28 proposed a bootstrap multiplier M(S) with cost-anchored emission. Neither was adopted in code — the protocol chose the simpler proportional emission model described in Section 3 above. The v28 proposal remains an interesting academic reference but does not reflect the protocol behavior.

---

## 13. References

1. Feng, Y., Liu, Z., Huang, J., Li, C., & Bhattacharjee, A. (2020). "Access Pattern-Aware DRAM Latency." _Proceedings of the VLDB Endowment_, 13(7), 898-911. https://www.vldb.org/pvldb/vol13/p898-feng.pdf

2. Jacob, B., Ng, S., & Wang, D. (2008). _Memory Systems: Cache, DRAM, Disk_. Morgan Kaufmann. ISBN 978-0-12-379751-3.

3. JEDEC Solid State Technology Association. JESD79-series standards: DDR3 (JESD79-3F), DDR4 (JESD79-4C), DDR5 (JESD79-5B). https://www.jedec.org/

4. Mutlu, O., & Subramanian, L. (2014). "Research Problems and Opportunities in Memory Systems." _Supercomputing Frontiers and Innovations_, 1(3). https://doi.org/10.14529/jsfi140301

5. Lee, D., et al. (2015). "Reducing DRAM Latency at Low Cost by Exploiting Heterogeneity." _Proceedings of the 48th International Symposium on Microarchitecture (MICRO-48)_. ACM.

6. Hassan, M., et al. (2017). "SoftMC: A Flexible and Practical Open-Source Infrastructure for Enabling Experimental DRAM Studies." _Proceedings of the 23rd International Symposium on High Performance Computer Architecture (HPCA)_. IEEE.

7. Kim, J.S., et al. (2014). "The DRAM Latency PUF: Quickly Estimating Physical Random-Access Memory Latency." _Proceedings of the 6th Workshop on Hot Topics in Memory (HotMemory)_.



---

*Ewatts Protocol v29 — June 2026*
