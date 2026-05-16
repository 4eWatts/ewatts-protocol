# Ewatts Protocol v3 — Formal Specification

**Memory-Bound Digital Currency**
*June 2026*

---

## 1. Introduction & Design Goals

Ewatts is a decentralized digital currency whose issuance is constrained by **verifiable memory bandwidth competition**. The protocol uses Memory-Bandwidth-Bound Proof-of-Work (MBPoW), where the bottleneck is DRAM bandwidth (improving ~1.2%/year) rather than transistor logic (improving ~100×/decade for SHA-256 ASICs).

Energy is not declared — it is inferred. Because DRAM access energy per bit is physically bounded and improves at <2%/year, sustained memory bandwidth is a direct proxy for energy expenditure. The protocol measures real memory movement, not claimed electricity consumption.

### Design Goals

| Goal | Description |
|------|-------------|
| Stable energy anchor | Energy→work correlation degrades <2%/year (vs >100×/decade for SHA-256) |
| ASIC resistance | Hardware advantage bounded at ~3-5× via DRAM bandwidth bottleneck |
| Non-governance | No dev funds, no voting, no keys. Immutable after genesis |
| Market-driven supply | Emission responds to committed bandwidth, not halving |
| Honest issuance | Every unit required real memory bandwidth to produce |
| Privacy by default | Ring signatures and stealth addresses |
| Oracle-free contract settlement | VR derives energy reference from on-chain data only |

### What This Protocol Is

Ewatts is a monetary system anchored to **irreducible memory movement**. Sustained DRAM bandwidth is the resource being competed for — a physical constraint that cannot be accelerated by transistor shrinks.

**What Ewatts is not:** Not an electricity tracker, not a stablecoin, not an energy receipt. **Ewatts is not a store of value** — gold and Bitcoin serve that role. The energy link is emergent (bandwidth × time → joules expended), not pegged. The VR provides a reference rate for contract settlement, not a price peg.

**Ewatts is a ruler.** Gold and Bitcoin appreciate relative to Ewatts; fiat currencies decay relative to Ewatts. Ewatts itself is designed to remain stable in real terms — anchored to the cost of energy production — so that credit markets in energy-denominated supply chains (agriculture, fertilizer, oil, electricity) can function without the inflation risk of fiat or the appreciation risk of gold.

---

## 1b. Year Zero — Bootstrapping

### The Problem

Before the first Ewatt has a market price, mining requires real expenditure: a DDR5 server (~$5,000) and electricity (~$100/month). No rational miner enters if the tokens mined are worth zero. Bitcoin solved this because Satoshi mined on a laptop with zero marginal cost. Gold did not need bootstrapping — it was already valued.

Ewatts has no pre-mine, no dev fund, no foundation allocation.

### The Solution — Founder Mining

The protocol founder deploys a DDR5 server in the first month and mines the Genesis Epoch alone. The cost is ~$5,000 in hardware + ~$300 in electricity over the first three months. This is funded by the founder personally.

**Public commitment**: The founder publishes the mining address and a signed statement: "I am mining the first blocks of the Ewatts network at my own expense. I commit not to sell any Ewatt mined before the network reaches block height 50,000." This is a personal pledge, not a protocol rule. The protocol has no mechanism to enforce it — but the founder's reputation is the collateral and the community can observe the address on-chain.

After three to six months:
- The supply is distributed across blocks (not concentrated at one timestamp)
- A liquidity pool is seeded with a portion of mined tokens (USDC/Ewatt pair)
- External miners can now observe a market price and make a rational entry decision

**Why this is not a pre-mine**: A pre-mine creates tokens before proof-of-work begins. Founder mining creates tokens *through* proof-of-work, at personal expense. The difference is fundamental.

### Bootstrap Risk: VR Manipulation in Low-Adoption Networks

During the bootstrap phase, a single miner with ~50% of total bandwidth can influence the VR through strategic over-declaration. Although the efficiency penalty (§3.5) applies when η < 0.7, the arithmetic of a small network still rewards manipulation:

- **Scenario**: Founder mines at 50 GB/s (honest) while an attacker declares 100 GB/s but delivers 50 GB/s (η = 0.5).
- **Penalty**: Effective commitment drops to 50 GB/s (c' = c × η).
- **Result**: Attacker receives equal reward to honest miner but inflated the VR numerator by 50 GB/s for that block, increasing VR by ~50% over the block window.
- **Cost to attacker**: Attacker pays for hardware capable of 100 GB/s but only uses 50 GB/s — a real cost, but potentially acceptable for a party with strategic interest in VR manipulation.

**Mitigations**:
1. The VR window (1,000 blocks) dilutes single-block manipulation.
2. The ramp-up period (10,000 blocks, ~70 days) caps single-miner bandwidth change to ±1% per block.
3. A formal game-theoretic analysis is required before mainnet (see §16.3).
4. For the bootstrap period specifically, consider a reduced VR window (100 blocks) or VR disclosure as informative only — contract settlement should not reference VR during the first 10,000 blocks.

---

## 2. Notation & Constants

### Notation

| Symbol | Meaning | Unit |
|--------|---------|------|
| c_i | Bandwidth commitment of miner i | GB/s |
| c_min | Minimum allowed commitment | GB/s |
| W_i | Work proven by miner i | GB processed |
| W_t | Total work in round | GB processed |
| η_i | Bandwidth efficiency of miner i | dimensionless |
| c'_i | Effective commitment after penalty/cap | GB/s |
| R | Emission rate for round | Ewatt |
| D | Difficulty target | accesses |
| N | Nonce | 64-bit |
| E | Epoch number | integer |
| VR | Valor de Referência | kWh/Ewatt |
| J_GB | Energy constant | 0.08 J/GB |
| kWh_J | Conversion | 3,600,000 J/kWh |

### Protocol Constants

| Constant | Value | Rationale |
|----------|-------|-----------|
| TARGET_BLOCK_TIME | 600 s | Bitcoin-compatible |
| BASE_EMISSION | 100 Ewatt/block | Initial emission |
| DAG_INITIAL_SIZE | 8 GB | Above CPU cache |
| DAG_GROWTH_RATE | 0.5 GB/year | ASIC hedge |
| DAG_EPOCH | 2,016 blocks | ~14 days |
| COMMIT_WINDOW | 1,000 blocks | Rolling median |
| RAMP_UP | 10,000 blocks | Initial protection |
| VERIFY_SAMPLE | 0.001 | 0.1% of accesses |
| ASIC_CONTINGENCY | 1.0 GB/year | If >5× for 2 yrs |
| EFFICIENCY_PENALTY | 0.7 | Below this → penalized |
| EFFICIENCY_CAP | 1.3 | Above this → capped |
| MIN_COMMIT | 1 GB/s | Absolute floor |
| VR_WINDOW | 1,000 blocks | Rolling window for VR |
| J_GB | 0.08 | Joules per GB DRAM access |
| MAX_CONTRACT_TENOR | 90 days | Maximum recommended settlement window |
| SETTLEMENT_TRANCHE_MIN | 14 days | Minimum recommended tranche interval |

---

## 3. Bandwidth Commitment Model

### 3.1 Principle

Miners declare **sustained DRAM bandwidth** (GB/s) rather than energy expenditure (kWh). Bandwidth is the resource actually consumed by the protocol — the DAG walk is purely memory-bound. Energy is the underlying cost (moving data through DRAM requires joules per bit), but it is inferred, not declared.

Bandwidth commitments are superior to energy declarations because bandwidth is:

- **Verifiable**: DAG walk work divided by time gives a direct measurement
- **Uniform**: 100 GB/s is 100 GB/s regardless of geography or electricity source
- **Physical**: the actual scarce resource in MBPoW
- **Auditable**: timing data embedded in proof traces enables independent verification

### 3.2 Energy Relationship

The energy cost of mining is a function of committed bandwidth:

```
joules_per_second ≈ c_i × J_GB × bits_per_access
```

Where `J_GB = 0.08 J/GB` (DDR5 baseline, fixed at genesis). This ratio is physically stable and requires no on-chain verification.

### 3.3 Commitment Message

Each miner submits a signed commitment alongside their block solution:

```
Commit = {
    miner_id:       bytes(32),   // Public key hash
    bandwidth_gbps: float64,     // Declared sustained bandwidth
    block_number:   uint64,      // Target block
    work_gb:        float64,     // GB processed (DAG walk)
    time_seconds:   float64,     // Wall clock duration
    signature:      bytes(64),   // Ed25519 signature over (bandwidth || block || work || time)
}
```

### 3.4 Minimum Commitment

```
c_min = max(MIN_COMMIT, 0.1 × median({c_i} over last COMMIT_WINDOW blocks))
```

During ramp-up (first 10,000 blocks), a smoothing function applies:

```
smoothing = block_height / RAMP_UP
c_max_change = 0.01 × smoothing
c'_i = clamp(c_i, c_prev × (1 - c_max_change), c_prev × (1 + c_max_change))
```

### 3.5 Efficiency and Effective Commitment

After each block, the protocol measures actual bandwidth delivered:

```
η_i = W_i / (c_i × Δt)

where:
  W_i = GB processed by miner i
  c_i = declared bandwidth (GB/s)
  Δt  = elapsed time since previous block
```

The effective commitment used for reward calculation:

```
if η_i < EFFICIENCY_PENALTY (0.7):  c'_i = c_i × η_i       // over-declaration penalized
if η_i > EFFICIENCY_CAP (1.3):      c'_i = c_i × 1.3       // under-declaration capped
otherwise:                           c'_i = c_i             // honest declaration
```

### 3.6 Example Scenarios

| Declared | Actual | η | Effective | Result |
|----------|--------|---|-----------|--------|
| 100 GB/s | 100 GB/s | 1.0 | 100 GB/s | Honest, no penalty |
| 150 GB/s | 100 GB/s | 0.67 | 100 GB/s | Over-declaration penalized (c' = c × η = 150 × 0.67 = 100) |
| 100 GB/s | 200 GB/s | 2.0 | 130 GB/s | Under-declaration capped (capping at 1.3 × 100 = 130) |
| 50 GB/s | 50 GB/s | 1.0 | 50 GB/s | Honest declaration, lower bandwidth → lower reward |

### 3.7 Probabilistic Bandwidth Audit

Proof traces include per-sample timestamps. Any full node can estimate:
- Access latency distribution
- Sustained throughput vs burst behavior
- Throttling events

Incentivized audits allow verifiers to request complete timing data for a block in exchange for a fraction of block reward (proposed: 0.1% of block reward per audit request, capped at 10 requests per block).

---

## 4. DAG Generation

### 4.1 Epoch

```
epoch = floor(block_number / DAG_EPOCH)
```

DAG regenerates every 2,016 blocks (~14 days). This prevents long-range precomputation.

### 4.2 DAG Size

```
size(epoch) = DAG_INITIAL_SIZE + floor(DAG_GROWTH_RATE × epoch × DAG_EPOCH / 52596)

where:
  blocks_per_year = 365.25 × 24 × 60 × 60 / TARGET_BLOCK_TIME = 52,596
```

| Time | Size | Notes |
|------|------|-------|
| Genesis | 8 GB | Above CPU cache capacity |
| +5 years | 10.5 GB | Commodity DDR5 still viable |
| +10 years | 13 GB | ASIC with fixed DRAM obsolete |
| +20 years | 18 GB | Commodity server adds DIMMs |

### 4.3 DAG Generation Algorithm

```
PROCEDURE generate_dag(epoch):
    seed = keccak256(epoch || GENESIS_HASH)
    cache_size = max(1, size_in_bytes / 128)    // ~1/128 of DAG

    // Initialize cache
    cache[0] = sha512(seed)
    for i = 1 to cache_size:
        cache[i] = sha512(cache[i-1])

    // Generate DAG elements
    dag = []
    for i = 0 to (size_in_bytes / 64):
        data = cache[i % cache_size]
        data = sha512(data XOR i)

        // Memory-hard mixing: 256 iterations with random cache access
        for j = 1 to 256:
            parent = fnv_hash(i XOR j, data[0..7]) % cache_size
            data = sha512(data XOR cache[parent])

        dag.append(data)

    return dag
```

### 4.4 DAG Generation Performance Target

8 GB in <60 seconds on commodity DDR5-4800, single-threaded. A community benchmark campaign is planned before mainnet launch. Reference implementations in Rust (optimized) and Python (correctness) will be published.

### 4.5 DAG Auto-Execution — ASIC Contingency

**Detection rule**: At the end of each DAG epoch (2,016 blocks), the protocol computes:

```
avg_η_epoch = mean({η_i for all miners across all blocks in the epoch})
```

**Trigger**: If `avg_η_epoch > 1.3` AND `mean_committed_bandwidth_per_block > MAX_COMMODITY_BANDWIDTH (100 GB/s)` for 2 consecutive years.

**Response**: DAG growth accelerates from 0.5 GB/year to 1.0 GB/year for the next epoch.

The thresholds are locked at genesis. No governance can change them. A hard-coded delay (one epoch = ~14 days) prevents false positives from transient network noise.

**Rationale**: If a miner is consistently achieving bandwidth efficiency above the commodity ceiling, they must have access to non-commodity hardware (ASIC, HBM, or near-memory compute). The DAG growth erodes that advantage by forcing larger working sets. This is physics-driven governance: the hardware itself votes.

---

## 5. Mining Algorithm

### 5.1 Mining Procedure

```
PROCEDURE mine(header, difficulty, dag):
    nonce = random_uint64()

    while true:
        mix = keccak256(header || nonce)
        walk_length = difficulty_to_accesses(difficulty)

        // Timing starts before DAG walk
        t_start = now()

        for i = 0 to walk_length - 1:
            index = mix[0..7] % len(dag)
            mix = sha512(mix XOR dag[index])

        t_end = now()

        result = keccak256(mix)
        if result <= difficulty:
            trace = generate_trace(nonce, walk_length, dag, header)
            return (nonce, trace, t_end - t_start)

        nonce += 1
```

### 5.2 Proof Trace (with Timing)

```
FUNCTION generate_trace(nonce, walk_length, dag, header):
    trace = []
    interval = max(1, walk_length × VERIFY_SAMPLE)   // ~0.1% sampling

    mix = keccak256(header || nonce)
    t_start = now()

    for i = 0 to walk_length - 1:
        index = mix[0..7] % len(dag)
        mix = sha512(mix XOR dag[index])

        if i % interval == 0:
            trace.append((i, index, mix, now() - t_start))

    return trace
```

### 5.3 Work Measurement

```
W_i = completed_accesses_i / BASE_ACCESSES

where BASE_ACCESSES = 10^9
```

Work is measured in units of GB processed. Each access reads one 64-byte element from the DAG.

### 5.4 Verification

Light verification (without full DAG):
1. Proof trace samples are checked for positional validity (consistent spacing)
2. Each sample's DAG index is verified against expected hash-computed position
3. Final hash must meet difficulty target
4. Timing data provides confidence in sustained bandwidth delivery

Full verification (with DAG):
1. Re-execute the sampled portions of the DAG walk
2. Verify each element access and mix update
3. Security assumption: forging a valid trace without the DAG requires sha512 collision, which is infeasible

---

## 6. Reward Calculation

### 6.1 Effective Commitment

As defined in §3.5, using efficiency measurement from each miner's block:

```
c'_i = f(η_i) × c_i    // see §3.5 for f(η)
```

### 6.2 Reward Distribution

```
reward_i = (c'_i / Σ c'_j) × R
```

Reward is proportional to effective bandwidth commitment share. The efficiency penalty (§3.5) already adjusts declared bandwidth for over/under-declaration, making additional work-weighting unnecessary. Σ(c'_i / Σ c'_j) = 1, so all emission is distributed.

### 6.3 Reward Properties

- **Proportional to effective bandwidth**: Higher effective commitment = higher reward. Single weight, no residual.
- **Zero-sum within round**: Σ reward_i = R
- **Deterministic**: Identical computation on any node
- **Incentive-compatible**: Under-declaration (η > 1.3) caps at 1.3× gain, providing negligible benefit. Over-declaration (η < 0.7) reduces effective commitment proportionally, reducing reward.

---

## 7. Emission Schedule

### 7.1 Linear Emission

```
R = BASE_EMISSION × (Σc_i / avg_over_window)

avg_over_window = mean(Σc_i over last COMMIT_WINDOW blocks)
```

Emission responds proportionally to total committed bandwidth. If total bandwidth doubles, emission doubles. No halving schedule, no predetermined supply cap.

### 7.2 Bounds

```
R_min = BASE_EMISSION × 0.1     // Floor: 10 Ewatt/block
R_max = BASE_EMISSION × 10.0    // Ceiling: 1,000 Ewatt/block
```

These bounds prevent extreme emission spikes during network attacks or temporary mining drops.

### 7.3 Supply Trajectory

Under stable conditions (Total_Commitment ≈ Historical_Average):

```
R ≈ BASE_EMISSION = 100 Ewatt/block
annual_supply ≈ 100 × 52,596 ≈ 5.26M Ewatt/year
```

Total supply is not predetermined. It emerges from actual mining activity.

---

## 8. Difficulty Adjustment

### 8.1 Formula

```
D_new = D_current × (target_accesses_per_round / actual_accesses_per_round)
```

### 8.2 Window

Adjusted every block using a 100-block moving window.

### 8.3 Bounds

```
0.5 × D_current ≤ D_new ≤ 2.0 × D_current
```

Prevents large difficulty swings from a single block.

### 8.4 Access-to-Difficulty Mapping

```
Required_Accesses = BASE_ACCESSES × (Difficulty / BASE_DIFFICULTY)

where BASE_ACCESSES is calibrated such that at BASE_DIFFICULTY,
a miner with 50 GB/s DRAM bandwidth takes approximately 600 seconds.
```

---

## 9. Block Structure

### 9.1 Block Header

| Field | Size | Description |
|-------|------|-------------|
| version | 4 B | Protocol version (0x0003) |
| previous_hash | 32 B | Parent block |
| merkle_root | 32 B | Transaction Merkle root |
| timestamp | 8 B | Unix epoch (seconds) |
| epoch | 8 B | DAG epoch |
| difficulty | 32 B | Target threshold |
| total_commit | 8 B | Σ bandwidth commitments |
| emission_rate | 8 B | R for this block |
| miner_commit | 8 B | Winning miner's declared bandwidth |
| nonce | 8 B | Proof-of-work nonce |
| elapsed_ms | 4 B | Mining duration in ms |
| vr_block | 8 B | VR at this block (kWh/Ewatt × 10^6) |
| proof_trace_size | 2 B | Number of proof trace entries |
| proof_trace | var | Access proof + timestamps |
| **Total** | ~112 + trace | — |

### 9.2 Block Body

| Field | Size | Description |
|-------|------|-------------|
| tx_count | 4 B | Number of transactions |
| transactions | var | Serialized transactions |
| commit_list | var | (miner_id, bandwidth_gbps, work_gb, time_s, signature) per miner |

### 9.3 VR Field

`vr_block` is included in the block header as a convenience field. Full nodes independently verify:

```
VR(block) = (Σ c'_i × Δt × J_GB) / (Σ reward_i × kWh_J)

where:
  Σ c'_i = total effective bandwidth committed in the VR_WINDOW
  Δt = 600 × VR_WINDOW (expected)
  J_GB = 0.08 J/GB
  kWh_J = 3,600,000 J/kWh
  Σ reward_i = total Ewatt issued in the VR_WINDOW
```

The field is informative, not authoritative. Any node can recompute it from historical data.

---

## 10. Transaction Structure

Standard ring-signature UTXO model. Details identical to v22 spec (§11).

- **Inputs**: previous_tx_hash, output_index, key_image
- **Outputs**: amount (8 B), stealth public key (33 B), optional Pedersen commitment (32 B)
- **Ring size**: 11 participants by default
- **Stealth addresses**: prevent address reuse
- **Voluntary audit proofs**: a user can reveal a specific transaction to an auditor without exposing all activity

---

## 11. VR — Valor de Referência

### 11.1 Definition

The VR (Valor de Referência) is an on-chain reference rate that expresses how many joules of proven energy expenditure were required per Ewatt issued in the recent epoch.

```
VR(block) = (Σ c'_i × Δt × J_GB) / (Σ reward_i × kWh_J)

where:
  c'_i = effective bandwidth commitment of miner i (GB/s)
  Δt = block time window (seconds)
  J_GB = 0.08 J/GB (DDR5 energy constant, fixed at genesis)
  reward_i = Ewatt issued to miner i
  kWh_J = 3,600,000 J/kWh

Units: kWh/Ewatt
```

### 11.2 Window

The VR is calculated over `VR_WINDOW = 1,000 blocks` (~7 days). This rolling window provides statistical confidence while remaining responsive to network changes.

### 11.3 Derivation

The VR is derived entirely from on-chain data:
1. Total effective bandwidth committed across the window (Σ c'_i from block headers)
2. Total Ewatt issued in the window (Σ reward_i from block headers)
3. The physical constant J_GB (0.08 J/GB), locked at genesis

No price feeds, no oracles, no exchange data, no off-chain computation.

### 11.4 VR Properties

| Metric | Value | Context |
|--------|-------|---------|
| Annualized volatility | 0.7-1.2% | Simulation across 7 market regimes |
| Update frequency | Every block (informative) | Exact on epoch boundaries |
| Deterministic | Yes | Same computation → same result |
| Oracle dependency | None | All inputs on-chain |
| Manipulation resistance | High | Would require sustained bandwidth manipulation over 1,000 blocks |

### 11.5 Comparison

| Instrument | Annualized Volatility | Oracle Required |
|------------|----------------------|-----------------|
| Ewatt VR | 0.7-1.2% | No |
| Gold (XAU/USD) | ~13% | Yes |
| EUR/USD | ~7% | Yes |
| USDT/USD | ~0.5% | Yes (reserve attestation) |
| Bitcoin/USD | ~60% | Yes |

### 11.6 VR Is Not a Price

**Critical distinction**: The VR expresses energy production cost, not market value. An Ewatt may trade at $10 USD on an exchange while VR indicates $0.05/kWh equivalent. The gap between spot price and VR is the "adoption premium" — the market's bet on future utility over current energy cost.

Two prices coexist:

- **Spot (Ewatt/USD)**: What the market believes Ewatt is worth. Reacts in seconds to news, adoption, speculation.
- **VR (kWh/Ewatt)**: What the network proves it cost to produce. Reacts in days, via bandwidth verification.

Unlike commodity markets (crude oil, natural gas), there is no forced convergence mechanism. You cannot buy Ewatt cheap on spot and burn it to mine — mining is proof-of-bandwidth, not proof-of-burn.

### 11.7 VR Gap Interpretation

| Condition | Interpretation |
|-----------|---------------|
| Spot >> VR | High speculative premium (most cryptocurrencies) |
| Spot ≈ VR | Commodity-energy maturity reached |
| Spot << VR | Undervalued vs production cost — unsustainable long-term |

### 11.8 Implementation

The VR is computed by each full node on demand. A reference implementation is provided in the `api/` module:

```
FUNCTION compute_vr(start_block, window=1000):
    total_energy_j = 0
    total_ewatt = 0

    for block in range(start_block - window, start_block):
        total_commit = block.total_commit    // Σ c'_i in GB/s
        block_time = block.timestamp - prev_block.timestamp
        total_energy_j += total_commit × block_time × J_GB
        total_ewatt += block.emission_rate

    return (total_energy_j / 3,600,000) / total_ewatt    // kWh per Ewatt
```

---

## 12. VR-Based Contract Settlement

### 12.1 Motivation

Token price volatility makes direct Ewatt-denominated contracts unreliable for commercial settlement. By denominating contracts in kWh (the real unit of energy) and settling in Ewatt at the VR of the settlement block, counterparties isolate themselves from token price speculation.

### 12.2 Contract Structure

Contracts are recorded on-chain as a special transaction type:

```
SettlementContract = {
    version:            uint16,       // 0x0001
    party_a:            bytes(32),    // Seller public key hash
    party_b:            bytes(32),    // Buyer public key hash
    kWh_amount:         float64,      // Denominated in kWh
    currency_denom:     string,       // Always "kWh"
    tenor_blocks:       uint32,       // Duration in blocks
    tenor_seconds:      uint32,       // Duration in seconds (alternative)
    settlement_block:   uint32,       // Block for settlement
    vr_anchor_block:    uint32,       // Reference block for VR computation
    parties_agreed_kwh: float64,       // Ewatt obligation at signing (informative)
    arbitrator:         bytes(32),    // Optional: third-party arbitrator
    state:              uint8,        // 0=created, 1=locked, 2=settled, 3=disputed
    signature_a:        bytes(64),    // Party A signature
    signature_b:        bytes(64),    // Party B signature
}
```

### 12.3 Settlement

At the settlement block:

```
Ewatt_obligation = kWh_amount / VR(settlement_block)
```

Party A (seller) delivers `Ewatt_obligation` Ewatt to Party B (buyer). The protocol does not enforce delivery — enforcement is outside the protocol's scope, as in any bilateral contract. The on-chain record provides an auditable, non-repudiable reference.

### 12.4 VR Timestamp

The VR is read at the block containing the settlement transaction. If settlement is not performed within `MAX_SETTLEMENT_WINDOW` (recommended: 6 blocks = ~1 hour) of the target settlement block, the VR from the block at which settlement is actually performed is used. This prevents gameability by delaying settlement.

### 12.5 Contract Slippage

Slippage arises from the divergence between VR (rolling ~7-day average) and instantaneous market conditions. Simulation results:

| Contract Tenor | USD-Denominated Slippage (median) | kWh-Denominated Slippage (median) | Tranched kWh Slippage (median) | Notes |
|----------------|-----------------------------------|------------------------------------|-------------------------------|-------|
| 7 days | 5-10% | <1% | — | Recommended for first-time counterparties |
| 14 days | 8-15% | <1% | — | Sweet spot for most B2B trade |
| 30 days | 12-20% | 2-5% | — | Acceptable for regular counterparties |
| 60 days | 18-35% | 5-10% | — | Use tranches (2×30d) |
| 90 days | 25-50% | 7-15% | 5-10% (3×30d tranches) | Not recommended without hedging or tranching |

**Key insight**: The 25-50% USD-denominated slippage is translation noise, not real purchasing power erosion. For counterparties whose costs and revenues are energy-denominated (fertilizer, soy, oil, electricity), the relevant metric is kWh-denominated slippage: 7-15% at 90 days. The claim "3-8%" sometimes cited for 90-day contracts is optimistic by approximately 2×; the validated range from Monte Carlo simulation across 7 market regimes is 7-15% for full-term settlement.

**Mitigation**: Tranched settlement (splitting into 3×30-day installments) reduces 90-day kWh slippage to 5-10%. Forward VR hedging is required for tenors ≥90 days without tranching.

### 12.6 Tranched Settlement

For long-duration contracts, parties should split settlement into tranches:

```
Contract: 100,000 kWh, 90 days

Tranche 1: 33,333 kWh settled at block + 30d (VR of block + 30d)
Tranche 2: 33,333 kWh settled at block + 60d (VR of block + 60d)
Tranche 3: 33,334 kWh settled at block + 90d (VR of block + 90d)
```

This reduces the per-tranche slippage from 25-50% to 12-20%.

### 12.7 Recommended Maximum Tenor by User Type

| User Type | Max Tenor | kWh Slippage at Max | Notes |
|-----------|-----------|---------------------|-------|
| Energy-denominated B2B (agri, fertilizer, oil) | 60 days (or 90d with tranches) | 5-10% | Costs track kWh. 90d requires tranching or forward hedge. |
| Mixed B2B (partially energy, partially fiat) | 30 days | 2-5% | Fiat exposure caps tenor. Forward VR hedge recommended above 30d. |
| Fiat-denominated SME | 14 days | <1% | Use as settlement rail, not store. Convert to fiat immediately. |
| Sovereign / reserve diversification | 90 days | 5-10% (tranched) | Large size justifies complexity. Monitor kWh PPP retention: -4% to -22% in 15-year simulations depending on regime (VR anchors to electricity cost, not broader CPI). |

---

## 13. Forward Contracts and Hedging

### 13.1 The Problem

Counterparties whose final obligations are in fiat (USD, BRL, EUR) face exchange rate risk even when contract terms are kWh-denominated. The VR protects against energy price divergence but not against fiat devaluation relative to Ewatt.

### 13.2 Forward VR Agreements

The deterministic VR formula enables forward pricing:

```
VR_forward(N) = VR(current) × (1 + ρ)^(N/1000)

where:
  N = number of blocks to settlement
  ρ = expected drift rate (historically 0.3-0.5% per 1000 blocks)
```

Two parties can enter a VR forward:
- Party A and B agree on `VR_forward(settlement_block)` = X kWh/Ewatt
- At settlement, if `actual_VR > X`, Party B pays Party A the difference
- If `actual_VR < X`, Party A pays Party B

The forward is a derivative, not a protocol feature. However, the VR's deterministic and low-volatility behavior (0.7-1.2% annualized) makes forward pricing feasible with standard Black-Scholes or jump-diffusion models.

### 13.3 Overcollateralized kWh-Pegged Asset (Future Extension)

Once Ewatt liquidity reaches critical mass, a secondary token pegged to kWh can be built:

- User deposits Ewatt as collateral (150-200% overcollateralization)
- Protocol mints a stable token pegged to kWh (e.g., "kWh-D")
- VR provides the real-time price feed
- If collateral ratio drops below 125%, position is liquidated

This is not in the current spec. It is a future extension that requires:
- Sufficient Ewatt liquidity for liquidation mechanisms
- Auditor nodes for collateral verification
- A robust liquidation market

### 13.4 Best Practice for B2B Users

| User Profile | Strategy |
|--------------|----------|
| Energy-denominated costs and revenues | kWh contracts ≤30d, VR-assisted settlement. Minimal fiat exposure. |
| Mixed (energy costs, fiat revenues) | kWh contracts ≤14d. Immediate USD conversion after settlement. |
| Fiat-denominated (service providers) | Use Ewatt as settlement rail only. Receive → convert to fiat within minutes. |

---

## 14. Bridging and Interoperability

### 14.1 Bridge Architecture

Ewatts is a standalone L1. Interoperability with other ecosystems requires a bridge:

```
┌──────────────┐     Bridge      ┌──────────────┐
│              │    Operator      │              │
│  Ewatts L1   │ ◄──────────────► │  Ethereum    │
│  (Native)    │    0.1-0.3% fee  │  (ERC-20)    │
└──────────────┘                  └──────────────┘
```

### 14.2 Wrapped Ewatt (wEWATT)

| Step | Ewatts L1 | Ethereum |
|------|-----------|----------|
| Lock | User sends Ewatt to bridge address | — |
| Mint | — | wEWATT minted 1:1 |
| Burn | — | User burns wEWATT |
| Unlock | Bridge sends Ewatt to user | — |

### 14.3 Bridge Operations

**Mint request** (Ewatt → wEWATT):
1. User sends native Ewatt to `bridge_lock_address` on Ewatts L1
2. After `CONFIRMATION_BLOCKS` (recommended: 6 blocks, ~1 hour), bridge operator observes the transaction
3. Bridge operator mints equivalent wEWATT on Ethereum
4. User receives wEWATT in their Ethereum wallet

**Burn request** (wEWATT → Ewatt):
1. User sends wEWATT to `bridge_burn_address` on Ethereum
2. Bridge operator verifies the burn transaction
3. Bridge operator releases native Ewatt from `bridge_lock_address` to user's L1 address
4. User receives native Ewatt

### 14.4 Bridge Fee

A fee of 0.1-0.3% is charged on each mint/burn operation. This covers:
- Ethereum gas costs for mint/burn transactions
- Bridge operator operational costs
- Buffer for slippage and timing risks

### 14.5 Bridge Trust Model

| Risk | Mitigation |
|------|------------|
| Bridge operator absconds with collateral | Transparency: `bridge_lock_address` is public, any user can verify collateral |
| Bridge operator halts operations | Decentralized bridge roadmap (see §14.6) |
| Smart contract bug on Ethereum | Audited contracts, time-locked upgrades, insurance fund |

### 14.6 Trustless Bridge (Future)

A trustless bridge using light client proofs over Ethereum is the long-term target:
- Ewatts light client (BFT header chain) deployed as Ethereum smart contract
- Users submit Merkle proofs of L1 transactions to the contract
- Contract verifies proofs and releases wEWATT

Development complexity is high (estimated 6-12 months). Short-term: trusted operator with transparency.

### 14.7 Fiat On/Off Ramps

Fiat access requires external integration:
- **CEX listing**: Exchange partners provide fiat trading pairs (Ewatt/BRL, Ewatt/USD)
- **Payment processor**: MoonPay, Onramp, or local alternatives for credit card / PIX purchase
- **P2P OTC**: Escrow-based desk for high-value B2B settlement

The protocol does not include fiat integration. It is the responsibility of the foundation and ecosystem partners.

### 14.8 No Forks

Ewatts integration is not achieved through forks. A fork creates a separate chain variant — fragmentation, not interoperability. All integration uses bridges, wrapped assets, and standard APIs.

---

## 15. Network Layer

Standard P2P design:
- **Peer discovery**: DNS seeds (hard-coded) + PEX (peer exchange)
- **Maximum connections**: 125 outbound
- **Message types**: `version`, `verack`, `addr`, `getblocks`, `inv`, `getdata`, `block`, `tx`, `commit`, `contract`
- **Block propagation**: Compact relay (announce → request → send → validate → re-announce)

### 15.1 Contract Message

The `contract` message type propagates settlement contract transactions (§12). Priority: lower than regular transactions (contracts have longer settlement windows, no urgency for propagation).

---

## 16. Game Theory

### 16.1 Dominant Strategy

The commitment system has a single stable equilibrium: declare sustained bandwidth accurately.

| Strategy | Result |
|----------|--------|
| Over-declare (200 GB/s, deliver 100 GB/s) | η=0.5, c'=100. Reward same as honest 100 GB/s, but hardware over-purchased |
| Under-declare (100 GB/s, deliver 200 GB/s) | η=2.0, c'=130. Only 30% gain, diminishing returns |
| Honest (150 GB/s, deliver 150 GB/s) | η=1.0, c'=150. Maximum reward for hardware deployed |

### 16.2 Cartel Resistance

Bandwidth is harder to collude around than energy claims because:
- The protocol measures actual throughput, not claimed cost
- A cartel cannot declare lower bandwidth than they deliver — the work reveals it
- Over-declaration (which reduces competition) actively harms cartel members' rewards

### 16.3 VR Gameability

Attacking the VR requires sustained bandwidth manipulation over 1,000 blocks:
- To inflate VR (make kWh/Ewatt appear higher): waste bandwidth → increases energy denominator → raises VR → bad for the attacker (costs real money)
- To deflate VR: reduce bandwidth → lowers VR → requires sustained attack across 1,000 blocks → economically irrational

The VR is not a price target. Manipulating it provides no direct profit to the attacker.

---

## 17. Attack Vectors

| Attack | Mitigation |
|--------|------------|
| Bandwidth over-declaration | Efficiency penalty (§3.5) |
| Burst vs sustained gaming | Timing-enabled continuous audit |
| 51% via DRAM botnet | Economic cost exceeds honest mining |
| ASIC advantage >5× | DAG growth acceleration contingency (2 year detection window) |
| Light verification grinding | Probabilistic sampling; full nodes verify 100% |
| Timewarp | ±2h timestamp window; median of 11 blocks |
| Long-range precomputation | DAG epoch (2,016 blocks); seed includes genesis hash |
| VR manipulation | Requires sustained bandwidth attack over 1,000 blocks; no profit incentive |
| Bridge operator fraud | Multi-signature governance; transparency reports; trustless bridge roadmap |

---

## 18. Implementation Architecture

### 18.1 Module Structure

```
ewatts/
├── core/
│   ├── dag.rs                   DAG generation and management
│   ├── proof.rs                 Proof-of-work algorithms
│   ├── block.rs                 Block structure and validation
│   ├── transaction.rs           Transaction structure and validation
│   ├── reward.rs                Reward calculation engine
│   ├── difficulty.rs            Difficulty adjustment
│   ├── commitment.rs            Bandwidth commitment validation
│   └── contract.rs              Settlement contract logic
├── vr/
│   ├── compute.rs               VR computation
│   ├── history.rs               VR historical data
│   └── forward.rs               Forward VR pricing (reference)
├── bridge/
│   ├── operator.rs              Bridge operator client
│   ├── proof.rs                 Merkle proof generation
│   └── contract_eth.sol         Ethereum ERC-20 contract
├── crypto/
│   ├── hash.rs                  Hash functions (keccak256, sha512)
│   ├── ring.rs                  Ring signatures
│   └── keys.rs                  Key generation
├── network/
│   ├── peer.rs                  Peer management
│   ├── protocol.rs              Message serialization
│   └── sync.rs                  Block synchronization
├── miner/
│   ├── engine.rs                Mining loop
│   ├── strategy.rs              Commitment strategy
│   └── dag_calc.rs              DAG computation
├── p2p/
│   ├── server.rs                P2P server
│   └── client.rs                P2P client
├── store/
│   ├── blockchain.rs            Block storage
│   ├── utxo.rs                  UTXO set
│   ├── contracts.rs             Contract store
│   └── state.rs                 Chain state
├── api/
│   ├── rpc.rs                   JSON-RPC interface
│   ├── wallet.rs                Wallet operations
│   └── vr_api.rs                VR query endpoints
└── dashboard/
    ├── vr_chart.rs              VR historical chart data
    ├── settlement.rs            Settlement UI data
    └── bridge_status.rs         Bridge monitor
```

### 18.2 API Extensions

Additional RPC methods beyond the JSON-RPC baseline:

| Method | Description |
|--------|-------------|
| `get_vr(block_number)` | Returns VR at specified block (or current) |
| `get_vr_history(from, to)` | Returns VR time series |
| `get_contract(hash)` | Returns contract state |
| `estimate_settlement(kWh, block)` | Estimates Ewatt obligation at future block |
| `get_bridge_status()` | Returns bridge collateral and pending operations |

### 18.3 Performance Targets

| Operation | Target | Hardware |
|-----------|--------|----------|
| DAG generation (8 GB) | <60 seconds | DDR5-4800, 16 GB |
| Block verification | <100 ms | Same |
| Transaction verification | <1 ms per input | Same |
| VR computation (1000 blocks) | <50 ms | Same |
| Contract verification | <2 ms | Same |
| Block propagation | <2 seconds (p95) | 100 Mbps |
| Full node sync | <2 hours | Same |

---

## 19. Business Model

### 19.1 Protocol Layer (Free)

The L1 protocol is public good: permissionless, open source, free. The protocol has no fees. Miners earn block rewards from emission. Transaction fees are optional (zero-fee transactions are valid if miner includes them).

### 19.2 Bridge Layer (Revenue)

The bridge operator charges 0.1-0.3% per mint/burn operation. At $10M monthly bridge volume: $10K-30K/month revenue. At $100M: $100K-300K/month.

### 19.3 Settlement UI (Revenue)

The contract settlement dashboard charges either:
- Per-contract fee: fixed fee per executed settlement ($5-50 depending on volume)
- SaaS subscription: monthly fee for B2B counterparties ($100-500/month per organization)

MVP phase: free to build adoption. Fee introduction at Phase 3 (see §23).

### 19.4 Explorer and VR Dashboard (Free)

Maintained as public good to maximize transparency and encourage third-party integration. Costs: hosting, maintenance, bandwidth. Funded from bridge and settlement revenue.

### 19.5 Cost Structure (Estimated Monthly)

| Item | Cost (USD) | Phase |
|------|------------|-------|
| Server infrastructure | $200-500/month | Bootstrap |
| Bridge operator gas costs | $500-2,000/month | Post-launch |
| Development (contractor) | $5,000-15,000/month | First 6 months |
| Legal/compliance | $2,000-5,000/month | As needed |
| **Total** | **$7,700-22,500/month** | — |

### 19.6 Founder Incentive

The founder mines the first blocks (§1b) and may use a portion of founder-mined Ewatt for:
- Seeding the Ewatt/USDC liquidity pool
- Funding bridge operations
- Paying development costs before bridge revenue covers them

This is transparent — the founder's mining address is public and auditable.

---

## 20. UX and Onboarding

### 20.1 Principles

1. **Default to hiding VR**: The typical user should never see the VR. The contract UI shows kWh amounts and Ewatt equivalents automatically.
2. **Default to simple**: One balance, one send button, one receive address. Wallets should abstract the DAG, MBPoW, and ring signatures.
3. **Progressive disclosure**: Advanced users (miners, B2B traders) can access VR charts, bandwidth analytics, and bridge controls.
4. **Localized**: Portuguese (primary target market), Spanish, English, Mandarin.

### 20.2 Required Infrastructure

| Component | Complexity | Who Builds |
|-----------|------------|------------|
| Reference wallet (desktop + CLI) | Medium | Foundation |
| Block explorer | Medium | Foundation (or third-party) |
| VR dashboard (historical chart, current rate) | Low | Foundation |
| Contract settlement UI | High | Foundation (or third-party) |
| Bridge frontend | Low | Foundation |
| Mobile wallet (Android/iOS) | High | Post-MVP |

### 20.3 KYC/AML Boundary

- **L1**: No KYC. Permissionless.
- **Bridge**: KYC above threshold (recommended: $10K/month wrap/unwrap)
- **Exchange**: Full KYC (exchange responsibility)
- **Settlement UI**: Optional KYC for regulated counterparties

---

## 21. Regulatory Framework

### 21.1 Jurisdictional Risk

Ewatts operates globally without a legal entity. This is a feature (neutrality) and a risk (no one to sue).

### 21.2 Classification by Jurisdiction

| Jurisdiction | Likely Classification | Risk |
|--------------|----------------------|------|
| United States (SEC) | Commodity (like Bitcoin). No ICO, no pre-mine, no dev fund. | Low |
| Brazil (CVM/BCB) | Commodity or payment instrument. | Low-moderate |
| EU (MiCA) | Crypto-asset; possibly ART if used for payments. | Moderate |
| China | Banned (all permissionless crypto). | High (irrelevant for target users) |
| Russia | Tolerated. Neutral settlement tool aligns with sanctioned economy. | Low to favorable |
| Switzerland | Compliant (foundation registration recommended). | Low |

### 21.3 AML/CFT

Privacy features (ring signatures, stealth addresses) create AML exposure for intermediaries, not for the protocol:
- **Exchanges**: Standard AML/KYC as required by local regulation
- **Bridge operator**: Transaction monitoring, suspicious activity reporting
- **L1**: No controls. This is consistent with Bitcoin and Monero.

### 21.4 Tax Treatment

General principles (jurisdiction-specific):
- **Mining**: Income at fair market value on receipt
- **Trading**: Capital gain/loss on disposal
- **Contract settlement**: Likely barter transaction (kWh → Ewatt)

Users should consult local tax professionals.

### 21.5 Recommended Approach

1. Register bridge entity in compliant jurisdiction (Switzerland or Singapore)
2. Maintain KYC/AML at bridge and exchange layers only
3. Publish monthly transparency reports for bridge collateral
4. Engage proactively with BCB and CVM in Brazil (primary target market)

---

## 22. Network Effects and Adoption

### 22.1 The Adoption Filter

Ewatts's kWh framing is both its strongest feature and adoption bottleneck. Users must understand (or trust) the energy-denominated mental model. This filters out speculators but aligns with:
- Agricultural commodity traders (soy, corn, fertilizer)
- Energy traders (oil, gas, electricity)
- Cross-border B2B in FX-restricted regimes
- Sovereign entities seeking neutral reserve diversification

### 22.2 MVP Definition

Ewatts is functional with:
1. 1+ miner (founder during bootstrap)
2. Reference wallet
3. Bridge to Ethereum testnet
4. Settlement UI for kWh-denominated contracts
5. 2-5 B2B counterparties in the same supply chain

---

## 23. Deployment Phases

| Phase | Timeline | Milestones |
|-------|----------|------------|
| 1. Bootstrap | Genesis + ~70 days (10K blocks) | Founder mining. Reference wallet. Explorer. VR dashboard. |
| 2. Permissionless | Post ramp-up | Open mining. Ethereum testnet bridge. Exchange listing discussions. |
| 3. Liquidity | Months 3-6 | Ethereum mainnet bridge. First CEX listing. 10-20 B2B counterparties. |
| 4. Scale | Months 6-12 | Major CEX listing. Payment processor. Settlement UI v1. |
| 5. Maturity | Year 2+ | Trustless bridge. DeFi integration. Sovereign engagement. |

---

## 24. Parameter Reference

| Parameter | Mainnet | Testnet |
|-----------|---------|---------|
| DAG initial size | 8 GB | 256 MB |
| DAG growth rate | 0.5 GB/yr | 0 |
| Block time | 600 s | 60 s |
| Base emission | 100 Ewatt/block | 10,000 |
| Ramp-up | 10,000 blocks | 100 blocks |
| Efficiency penalty | η < 0.7 | η < 0.7 |
| Efficiency cap | η > 1.3 | η > 1.3 |
| Commit window | 1,000 blocks | 100 blocks |
| VR window | 1,000 blocks | 100 blocks |
| Verification sample | 0.1% | 1.0% |
| Ring size | 11 | 5 |
| Min commit | 1 GB/s | 0.1 GB/s |
| J_GB constant | 0.08 J/GB | 0.08 J/GB |
| ASIC contingency | 1.0 GB/yr (if triggered) | 1.0 GB/yr |
| ASIC detection window | 2 years | N/A (testnet) |
| Settlement max tenor | 90 days | 30 days |

---

## 25. Risks and Limitations

1. **Contract slippage**: 25-50% USD translation risk on 90d contracts. Mitigated by short tenors and kWh-denominated settlement (§12.5). Real kWh-denominated slippage is 7-15% at 90d (not 3-8% as initially modeled — validated via Monte Carlo). Tranched settlement reduces this to 5-10%.

2. **kWh PPP retention asymmetry**: Over 15 years, kWh-denominated purchasing power retention ranges from -4% (favorable regimes) to -22% (Energy Crisis Persistent). The VR anchors to electricity cost, not to the broader economy. A reserve holder in a high-inflation energy regime loses real purchasing power even when denominating in kWh. This is a structural limitation for sovereign reserve use cases.

3. **Bootstrap VR manipulation**: In low-adoption networks, a single miner with ~50% of bandwidth can influence the VR through strategic over-declaration. Partially mitigated by VR window (1,000 blocks) and ramp-up caps, but formal game-theoretic analysis is pending (§1b).

4. **Adoption filter**: The kWh mental model is narrow. Scaling requires onboarding counterparties who think in energy terms.

5. **Bridge trust**: Current design requires a trusted operator. Trustless bridge is high-cost. The bridge operator is a single point of failure and a potential sanctions target — directly conflicting with the protocol's positioning for sanction-exposed supply chains.

6. **Sanctions exposure of bridge operator**: Because Ewatts targets parties under FX restrictions and sanction exposure, the bridge operator becomes a natural enforcement target. Any regulated entity running the bridge may be compelled to block transactions involving sanctioned jurisdictions.

7. **Regulatory uncertainty**: Privacy features invite scrutiny. KYC/AML at bridge layer may not satisfy all regulators.

8. **ASIC development**: Unproven ASIC advantage >5× could outpace DAG growth compensation. Two-year detection window is conservative but slow.

9. **No governance**: Immutability is a feature. If a critical bug is discovered, there is no fix mechanism. The protocol is what it is at genesis.

10. **VR ≠ price stability**: The VR is a production cost reference, not a price peg. Confusing the two leads to risk mismanagement (§11.6).

---

*Ewatts Protocol v3 — Formal Specification*
*June 2026*
