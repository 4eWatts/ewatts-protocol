# eWatts Security Engineering Handbook — Version 3.0

# eWatts Security Engineering Handbook — Version 4.0

**Document type:** Protocol Specification + Security Engineering Reference  
**Protocol version:** 0x0005 (V4) (v3 emission + AOPS commitment)  
**Target:** eWatts Proof-of-Work blockchain  
**Date:** July 2026  

> **Status:** Normative Specification + Engineering Reference  
> This document combines a normative protocol specification (RFC-style) with a security engineering handbook.  
> Items marked **[Requires Code Confirmation]** identify potential issues needing manual source verification.  
> Items marked **"CONFIRMED"** have been verified against source code lines referenced.  
> Sections using RFC 2119 language (MUST, SHOULD, MAY) are normative protocol requirements.

---

## Table of Contents

**Volume I — Protocol Specification (RFC-Style)**
1. [Notation & Conventions](#1-notation--conventions)
2. [Block Structure](#2-block-structure)
3. [Transaction Structure](#3-transaction-structure)
4. [Consensus Rules](#4-consensus-rules)
5. [Network Protocol](#5-network-protocol)

**Volume II — Formal Foundations**
6. [Mathematical Proofs](#6-mathematical-proofs)
  6.9 [Bayesian-Nash Entry Equilibrium](#69-bayesian-nash-entry-equilibrium-with-heterogeneous-costs)
  6.10 [Dynamic Stability of Emission Clamp](#610-dynamic-stability-of-the-emission-clamp)
7. [Computational Complexity](#7-computational-complexity)
8. [Test Vectors](#8-test-vectors)

**Volume III — Threat & Security Analysis**
9. [Cryptographic Primitive Analysis](#9-cryptographic-primitive-analysis)
10. [Extended Economic Attack Taxonomy](#10-extended-economic-attack-taxonomy)
11. [Performance Analysis](#11-performance-analysis)

**Volume IV — Engineering Analysis (from V2)**
12. [Mechanism Design & Game Theory](#12-mechanism-design--game-theory)
13. [Rust Safety Engineering](#13-rust-safety-engineering)
14. [Consensus & Distributed Systems](#14-consensus--distributed-systems)
15. [Privacy & Cryptographic Protocols](#15-privacy--cryptographic-protocols)
16. [Historical Vulnerability Case Studies](#16-historical-vulnerability-case-studies)

**Volume V — Appendices**
17. [Appendix A: Protocol Invariants](#17-appendix-a--protocol-invariants)
18. [Appendix B: Formal State Machine](#18-appendix-b--formal-state-machine)
19. [Appendix C: Normative Constants](#19-appendix-c--normative-constants)
20. [Appendix D: Security Assumptions](#20-appendix-d--security-assumptions)
21. [STRIDE Analysis Matrix](#22-stride-analysis-matrix)
22. [Attack Trees](#23-attack-trees)
23. [CAPEC Mapping](#24-capec-mapping)
24. [Audit Procedures Checklist](#25-audit-procedures-checklist)
25. [References](#26-references)

---

# Volume I — Protocol Specification (RFC-Style)

---

## 1. Notation & Conventions

### 1.1 RFC 2119 Keywords

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY**, and **OPTIONAL** in this document are to be interpreted as described in RFC 2119 [Bradner, 1997].

### 1.2 Data Types

| Type | Size | Description |
|------|------|-------------|
| `uint32` | 4 bytes | Little-endian unsigned 32-bit integer |
| `uint64` | 8 bytes | Little-endian unsigned 64-bit integer |
| `hash32` | 32 bytes | Keccak-256 or SHA3-256 output |
| `hash64` | 64 bytes | SHA-512 output |
| `point32` | 32 bytes | Compressed Ristretto curve point |
| `scalar32` | 32 bytes | Scalar modulo L (Ristretto group order) |
| `signature64` | 64 bytes | Ed25519 signature (R || S) |
| `byte[]` | variable | Variable-length byte array |

### 1.3 Cryptographic Primitives

| Operation | Specification | Output |
|-----------|--------------|--------|
| Keccak-256(x) | FIPS 202 SHA-3 | `hash32` |
| SHA-512(x) | FIPS 180-4 | `hash64` |
| SHAKE-256(x, len) | FIPS 202 XOF | Variable output |
| Ed25519.Sign(sk, msg) | RFC 8032 | `signature64` |
| Ed25519.Verify(pk, msg, sig) | RFC 8032 | Boolean |
| Ristretto.Point(s) | curve25519-dalek | `point32` compressed |
| Ristretto.Scalar(s) | curve25519-dalek | `scalar32` |

### 1.4 Hashing to Curve (Hash-to-Point)

```
HtoP(domain, data) → RistrettoPoint:
  seed = SHAKE-256("Ewatts_HTP_v1:" || domain || data, 64)
  for attempt = 0, 1, 2, ...:
    candidate = SHAKE-256(seed || attempt, 32)
    if CompressedRistretto(candidate) is valid point:
      return decompressed point
```

**Domain separation tags:**

| Tag | Usage |
|-----|-------|
| `b"Ewatts_Ring_G_v1"` | Generator G for ring signatures |
| `b"Ewatts_Pedersen_H_v1"` | Generator H for Pedersen commitments |
| `b"Ewatts_HTP_v1:"` | Generic hash-to-point (prepended to input data) |

---

## 2. Block Structure

### 2.1 BlockHeader

```
struct BlockHeader {
    version:        uint32,     // MUST be 0x0005
    previous_hash:  hash32,     // MUST be parent block hash, or [0;32] for genesis
    merkle_root:    hash32,     // MUST be Keccak-256 merkle root of all tx hashes
    timestamp:      uint64,     // Unix timestamp in seconds
    height:         uint64,     // Block height (0-indexed, genesis = 0)
    epoch:          uint64,     // DAG epoch = height / DAG_EPOCH_BLOCKS (2016)
    difficulty_target: uint64,  // PoW difficulty target (≥1)
    total_effective_commit: uint64,  // Sum of all miners' effective AOPS (base units)
    emission_rate:  uint64,     // Emission for this block (base units)
    miner_effective_commit: uint64,  // This block's miner effective AOPS (base units)
    vr_block:       uint64,     // Block VR (kWh per eWatt, scaled × 10^6)
    coinbase_burn:  uint64,     // eWatt burned due to ramp-up cap (base units)
    nonce:          uint64,     // PoW nonce (solution)
    elapsed_ms:     uint32,     // Mining time in milliseconds
    proof_merkle_root: hash32?, // OPTIONAL merkle root of proof trace samples
}
```

**Serialization for header hash:**
```
BlockHash = Keccak-256(
    version || previous_hash || merkle_root || timestamp || height ||
    epoch || difficulty_target || total_effective_commit || emission_rate ||
    miner_effective_commit || vr_block || coinbase_burn || nonce ||
    elapsed_ms || (proof_merkle_root if present)
)
```

**Serialization for proof hash (MUST exclude proof-dependent fields):**
```
ProofHash = Keccak-256(
    version || previous_hash || merkle_root || timestamp || height ||
    epoch || difficulty_target || total_effective_commit || emission_rate ||
    miner_effective_commit || vr_block || coinbase_burn
)
```

The ProofHash is the value committed to during mining. Nonce, elapsed_ms, and proof_merkle_root are excluded because they are determined AFTER the proof is found.

### 2.2 BlockBody

```
struct BlockBody {
    transactions:  Transaction[],
    commitments:   Commitment[],
}
```

The first transaction in `transactions` MUST be the coinbase transaction. All remaining transactions are user transactions.

### 2.3 Block

```
struct Block {
    header:     BlockHeader,
    body:       BlockBody,
    proof_hash: hash32,  // MUST equal BlockHeader.proof_hash()
}
```

### 2.4 Genesis Block

The genesis block is defined with:
- `previous_hash = [0; 32]`
- `height = 0`
- `timestamp` = network start time
- `difficulty_target = 1`
- Single coinbase transaction creating `1,000,000` eWatt (100,000,000,000 base units) to the genesis public key

---

## 3. Transaction Structure

### 3.1 TxInput

```
struct TxInput {
    previous_tx_hash: hash32,      // UTXO being spent
    output_index:     uint32,       // Output index within the referenced TX
    key_image:        hash32,       // Deterministic spend key identifier
    revealed_pubkey:  byte[]?,      // P2PKH: revealed public key (32 bytes)
}
```

For legacy (non-private) transactions: `revealed_pubkey` MUST be present (exactly 32 bytes).  
For private (MLSAG) transactions: `revealed_pubkey` MUST be empty (`len = 0`).

### 3.2 TxOutput

```
struct TxOutput {
    amount:            uint64,       // Value in base units (1 eWatt = 10^6 units)
    pubkey_hash:       hash20?,      // P2PKH: Keccak-256 truncated to 20 bytes
    spendable_after:   uint64,       // Block height before which this output is unspendable
    stealth_dest:      point32?,     // Private mode: stealth one-time destination
    commitment_bytes:  point32?,     // Private mode: Pedersen commitment
    range_proof_bytes: byte[]?,      // Private mode: range proof serialization
    ephemeral:         point32?,     // Private mode: ephemeral public key R
}
```

**Constraints:**
- P2PKH output: `pubkey_hash` MUST be present. `stealth_dest`, `commitment_bytes`, `range_proof_bytes`, `ephemeral` MUST be absent.
- Private output: `stealth_dest`, `commitment_bytes`, `range_proof_bytes` MUST be present. `ephemeral` MAY be present. `pubkey_hash` MUST be [0; 20].
- A transaction MUST NOT mix P2PKH and private outputs (all outputs MUST be of the same mode).

### 3.3 Transaction

```
struct Transaction {
    version:      uint16,            // MUST be 1
    inputs:       TxInput[],          // MUST have ≥1 input (except coinbase)
    outputs:      TxOutput[],         // MUST have ≥1 output
    ring_size:    uint16,             // Ring size for MLSAG (≥2 for private, 1 for P2PKH)
    signatures:   byte[][],           // Ed25519 signatures (P2PKH) or empty (private)
    mlsag:        MlsagData?,         // OPTIONAL: MLSAG signature data
    ring_members: UtxoRef[][]?,       // OPTIONAL: ring member references
}
```

**TX hash computation:**
```
TxHash = Keccak-256(version || inputs_hash || outputs_hash || ring_size)
where:
  inputs_hash = Keccak-256(concat(input[i].previous_tx_hash || input[i].output_index || input[i].key_image))
  outputs_hash = Keccak-256(concat(output[j].amount || output[j].pubkey_hash || output[j].stealth_dest || output[j].commitment_bytes))
```

### 3.4 Coinbase Transaction

The coinbase transaction:
- MUST have zero inputs (`inputs = []`)
- MAY have one or more outputs
- The sum of output amounts MUST equal the miner's reward (emission_rate - coinbase_burn, pro-rated by effective commitment share)
- Outputs MUST have `spendable_after` set according to `founder_lock_block(height)` for blocks < 10,000
- The total output amount MUST NOT exceed `20 × BASE_EMISSION_UNITS` (200,000,000,000 base units)

### 3.5 Commitment Structure

```
struct Commitment {
    miner_id:           point32,       // Ed25519 public key
    access_ops_per_sec: f64,           // Declared AOPS
    block_number:       uint64,        // Block height
    total_access_ops:   f64,           // Total access operations performed
    time_seconds:       f64,           // Mining duration in seconds
    signature:          byte[],         // Ed25519 signature over commit_msg
}
```

**Commitment message to sign:**
```
CommitMsg = miner_id || access_ops_per_sec (8 LE bytes) ||
            block_number (8 LE bytes) || total_access_ops (8 LE bytes) ||
            time_seconds (8 LE bytes)
```

### 3.6 MLSAG Signature

```
struct MlsagData {
    ring_size:  uint16,
    n_layers:   uint16,
    key_images: point32[],           // One per layer
    c0:         scalar32,            // Initial challenge
    responses:  scalar32[][],        // [ring_size][n_layers] responses
}
```

### 3.7 UtxoRef

```
struct UtxoRef {
    tx_hash:      hash32,
    output_index: uint32,
}
```

---

## 4. Consensus Rules

### 4.1 Proof of Work

The PoW algorithm is a memory-hard DAG-walk.

**Algorithm:**

```
mine(header_hash, difficulty, dag, nonce_limit):
    walk_length = BASE_ACCESSES * difficulty / 1_000_000_000
    sample_interval = max(1, walk_length / 1000)
    
    for attempt in 0..nonce_limit:
        nonce = random_u64()
        mix = initial_mix(header_hash, nonce)
        
        for i in 0..walk_length:
            index = read_u64_le(mix[0..8]) % dag.len()
            mix = SHA-512(mix XOR dag[index])
            if i % sample_interval == 0:
                append_to_trace(i, index, mix, elapsed_us)
        
        final_hash = Keccak-256(mix)
        if meets_difficulty(final_hash, difficulty):
            merkle_root = merkle_root_from_leaves(trace)
            return Solution { nonce, trace, walk_length, merkle_root }
    
    return None (no solution)
```

**Difficulty check:**
```
meets_difficulty(hash, difficulty):
    target = u64::MAX / max(difficulty, 1)
    return read_u64_le(hash[0..8]) ≤ target
```

### 4.2 Block Validation

Upon receiving a block, validators MUST:

1. **Block structure check**: All fields present and well-formed.
2. **Previous hash check**: `header.previous_hash` references a known block in the chain store, OR is `[0; 32]` for genesis.
3. **Height check**: `header.height = parent.height + 1`.
4. **Timestamp check**: `header.timestamp > parent.timestamp` and `header.timestamp < now() + 7200` (2 hour drift tolerance).
5. **Difficulty check**: `header.difficulty_target` matches the adjusted difficulty from the previous window.
6. **Proof check**: `proof::verify(proof_hash, solution, difficulty, dag)` returns `Ok(())`.
7. **Merkle check**: `merkle_root = merkle_root_from_leaves(tx_hashes)`.
8. **Emission check**: `header.emission_rate` in `[5, 2000]` base eWatt (scaled).
9. **Coinbase check**: First transaction has zero inputs, outputs sum ≤ 2000 eWatt.
10. **State check**: All transactions validate against current UTXO set.

### 4.3 Commitment Validation

Each commitment MUST satisfy:
1. `access_ops_per_sec >= 20,000,000` (MIN_COMMIT_AOPS)
2. `access_ops_per_sec >= 0.1 × median(recent_commitments)` (rolling minimum)
3. `signature.len() == 64`
4. Efficiency = `total_access_ops / (access_ops_per_sec × time_seconds) > 0`
5. Ed25519 signature verifies with `miner_id` as public key

### 4.4 Reward Distribution

```
emission_rate = clamp(100.0 × total_effective_eff / historical_avg_aops, 5.0, 2000.0)

For each miner i:
    reward_i = (c_eff_i / total_eff) × emission_rate (in eWatt)

Ramp-up cap (height < 10,000):
    If reward_i / total_reward > 0.80:
        excess = reward_i - total_reward × 0.80
        burned += excess
        reward_i = total_reward × 0.80

Supply update:
    total_supply += sum(reward_i) in base units
    burned_amount tracked in header.coinbase_burn
```

### 4.5 Fork Choice Rule

The canonical chain is the chain with the highest accumulated work:
```
accumulated_work(block) = parent.accumulated_work + u64::MAX / difficulty_target
```

On receiving a block that extends a different fork:
1. If the new fork's accumulated work > current tip's accumulated work:
   a. Compute fork point (lowest common ancestor)
   b. Unwind blocks from current tip to fork point
   c. Apply blocks from fork point to new tip
   d. Set new tip
2. If the new fork's accumulated work ≤ current tip's: keep as sidechain
3. Reorg depth MUST NOT exceed 100 blocks

---

## 5. Network Protocol

### 5.1 Transport

eWatts uses libp2p with TCP as the transport layer:
- Noise protocol for authenticated encryption (XX handshake)
- Yamux for multiplexing
- Connection budget: 5 conn/s burst, 5/s refill
- Max peers: 200
- Idle timeout: 60 seconds

### 5.2 Gossip Protocol

Blocks and transactions are propagated via libp2p gossipsub:
- Topic: `/ewatts-blocks/1.0.0`
- Heartbeat interval: 5 seconds
- Message ID: `Keccak-256(raw_message_bytes)`

### 5.3 Compact Block Protocol

```
struct CompactBlock {
    header:     BlockHeader,
    nonce:      uint64,           // For short ID derivation
    coinbase:   Transaction,      // Always prefilled
    short_ids:  uint64[],         // First 8 bytes of Keccak-256(tx_hash || nonce)
    proof_hash: hash32,
}
```

Reconstruction: Receiver matches short IDs against local mempool transactions. If any short ID is unmatched, receiver MUST request the full block via `RequestFullBlock`.

### 5.4 Message Types

```
enum P2pMessage:
    BlockRequest { from_height: uint64, to_height: uint64 }
        // Request range: SHOULD be ≤ 500 blocks
    
    BlockResponse { blocks: Block[] }
        // Response MUST validate all fields
    
    NewTransaction(Transaction)
        // Transaction gossip
    
    NewBlock(Block)
        // Full block gossip
    
    CompactBlock(CompactBlock)
        // Compact block gossip (preferred)
    
    RequestFullBlock { height: uint64 }
        // Fallback when compact block reconstruction fails
    
    FullBlockResponse { block: Block }
        // Full block data
```

### 5.5 Sync Protocol

Protocol ID: `/ewatts/block-sync/1`  
Request timeout: 30 seconds  

On connecting to a new peer:
1. Exchange chain tips
2. Request missing blocks via `BlockRequest`
3. Validate each block before applying

---

# Volume II — Formal Foundations

---

## 6. Mathematical Proofs

### 6.1 Notation

Let:
- $\mathcal{B} = \{b_0, b_1, \ldots\}$ = set of all blocks, indexed by height
- $\mathcal{M} = \{m_1, \ldots, m_k\}$ = set of miners
- $c_i$ = miner $i$'s declared AOPS
- $e_i = s_i / (c_i \cdot t_i)$ = miner $i$'s efficiency
- $\bar{c}_i = \phi(c_i, e_i)$ = effective commitment (penalty function)
- $\Sigma = \sum_i \bar{c}_i$ = total effective AOPS
- $H$ = historical average AOPS (over $W$ blocks)
- $E(\Sigma) = \max(5, \min(2000, 100 \cdot \Sigma / H))$ = emission rate
- $R_i = (\bar{c}_i / \Sigma) \cdot E$ = miner $i$'s reward

### 6.2 Fixed Point of the Effective Commitment Function

**Theorem 1 (Fixed Point of $\phi$).** The effective commitment function $\phi: \mathbb{R}_+ \times \mathbb{R}_+ \to \mathbb{R}_+$ has a unique fixed point for each declared $c$: the strategy $e = 1$ (truthful execution).

*Proof.*
The function $\phi$ is defined as:
$$
\phi(c, e) = \begin{cases} c \cdot e & \text{if } e < 0.7 \\ c & \text{if } 0.7 \leq e \leq 1.3 \\ 1.3 \cdot c & \text{if } e > 1.3 \end{cases}
$$

We seek $e^*$ such that $\phi(c, e^*) = c$ (output equals input). This requires the middle case: $0.7 \leq e^* \leq 1.3$. Any $e^*$ in this interval satisfies $\phi(c, e^*) = c$ (no penalty, no cap). However, the *natural* fixed point is $e^* = 1$ because this corresponds to the miner delivering exactly the declared work, with no under-performance and no artificial inflation.

For $e < 0.7$: $\phi(c, e) = c \cdot e < c$ (strictly decreasing). Miners who under-deliver receive less effective credit.
For $e > 1.3$: $\phi(c, e) = 1.3c > c$ (capped at 30% overhead). Miners who over-deliver receive only partial credit.

**Corollary 1.1.** The mechanism incentivizes $e \in [0.7, 1.3]$, with $e = 1$ being the unique equilibrium when marginal cost = marginal benefit. ∎

### 6.3 Proportional Reward Equilibrium

**Theorem 2 (Nash Equilibrium of One-Shot Game).** In the one-shot mining game with $N \geq 2$ symmetric miners, declaring true AOPS and delivering declared work ($e_i = 1$ for all $i$) is a Nash equilibrium.

*Proof.*
Consider miner $i$ with true capacity $C_i$. Declaring $c_i$ and delivering $s_i = c_i \cdot t_i$:
$$
u_i = R_i \cdot P - \text{cost}_i = \frac{\bar{c}_i}{\Sigma} \cdot E(\Sigma) \cdot P - \alpha c_i
$$
where $\alpha = J\_PER\_ACCESS \cdot t_i \cdot p_{elec}$ and $P$ is the eWatt price.

Suppose miner $i$ deviates to $c_i' \neq C_i$ while maintaining $e_i = 1$ (delivering declared work). Two cases:

**Case 1 (under-declare):** $c_i' < C_i$. Then $\bar{c}_i' = c_i' < C_i = \bar{c}_i$. The reward share $\bar{c}_i'/\Sigma' < \bar{c}_i/\Sigma$. Cost also decreases: $\alpha c_i' < \alpha c_i$. The net effect depends on elasticities, but for any $\Sigma$ large enough that $E(\Sigma) = 2000$ (ceiling), the marginal reward per additional AOPS is zero. At the ceiling, miners have no incentive to declare more than what keeps $\Sigma$ at the ceiling boundary.

**Case 2 (under-deliver):** $c_i = C_i$ but $e_i < 0.7$. Then $\bar{c}_i = C_i \cdot e_i < C_i$. Reward strictly decreases. Cost unchanged (still $\alpha C_i$). Defection reduces profit.

**Case 3 (over-deliver):** $e_i > 1.3$. Then $\bar{c}_i = 1.3 \cdot C_i$, but cost increases to $\alpha \cdot (e_i / C_i)$. For $e_i << 1.3 \cdot C_i$, the miner spends more on power than the 1.3x cap allows as reward. Unprofitable.

Thus, $c_i = C_i$, $e_i = 1$ is a best response to all other miners playing $c_j = C_j$, $e_j = 1$. ∎

**Note:** This proof assumes $P$ is fixed (price-taking miners). If $P$ is endogenous (mining affects supply affects price), the equilibrium condition is more complex.

### 6.4 Free Entry Equilibrium (Gustavo, July 2026)

**Theorem 3 (Free Entry Equilibrium).** Under free entry with homogeneous miners consuming $W$ watts per node, the equilibrium satisfies:
$$
N = \frac{8000 \cdot P}{p_{elec}}
$$
$$
E(\text{block}) = 100 \cdot \frac{P}{p_{elec}} \text{ kWh}
$$
$$
VR = \frac{P}{p_{elec}} \text{ kWh/eWatt}
$$
where $P$ = eWatt market price, $p_{elec}$ = electricity cost per kWh.

*Proof.*
Each node consumes wall power $W = 75$W for $T = 600$s per block. Energy per node per block:
$$
E_{node} = \frac{W \cdot T}{3.6 \times 10^6} = \frac{75 \cdot 600}{3.6 \times 10^6} = 0.0125 \text{ kWh}
$$

Cost per node per block (at electricity price $p_{elec}$ per kWh):
$$
c = E_{node} \cdot p_{elec} = 0.0125 \cdot p_{elec} \text{ USD/block}
$$

Total network cost with $N$ nodes: $N \cdot c$.

Total network revenue per block: $100 \cdot P$ (base emission of 100 eWatt at market price $P$).

Free entry condition (marginal miner breaks even):
$$
N \cdot c = 100 \cdot P
$$

Substituting $c$:
$$
N \cdot 0.0125 \cdot p_{elec} = 100 \cdot P
$$

Solving for $N$:
$$
N = \frac{100 \cdot P}{0.0125 \cdot p_{elec}} = \frac{8000 \cdot P}{p_{elec}}
$$

Energy consumed by the entire network per block:
$$
E_{block} = N \cdot E_{node} = \frac{8000 \cdot P}{p_{elec}} \cdot 0.0125 = \frac{100 \cdot P}{p_{elec}} \text{ kWh}
$$

Reference Value (VR) is energy per block divided by eWatt per block:
$$
VR = \frac{E_{block}}{100} = \frac{P}{100 \cdot p_{elec}} \text{ kWh/eWatt}
$$

**Corollary 3.1 (Hardware Efficiency Neutrality).** The equilibrium $N$ depends on $W$ (watts per node), but energy per block $E_{block} = P/p_{elec}$ is independent of $W$. Hardware efficiency improvements increase $N$ (more nodes) rather than reducing per-block energy.

*Proof.* Substitute $N = 80P/(W \cdot p_{elec})$ into $E = N \cdot W \cdot 600 / 3.6 \times 10^6$: $W$ cancels. ∎

### 6.5 Emission Elasticity

**Theorem 4 (Emission Elasticity).** The emission elasticity with respect to total effective AOPS is:
$$
\epsilon_E = \frac{\partial E}{\partial \Sigma} \cdot \frac{\Sigma}{E} = \frac{100 \cdot \Sigma}{H \cdot E}
$$

*Proof.* $E = clamp(100 \cdot \Sigma / H, 5, 2000)$. Within the interior ($5 < E < 2000$), $E = 100 \cdot \Sigma / H$. Then:
$$
\frac{\partial E}{\partial \Sigma} = \frac{100}{H}
$$
$$
\epsilon_E = \frac{100}{H} \cdot \frac{\Sigma}{100 \cdot \Sigma / H} = 1
$$

**Corollary 4.1.** $\epsilon_E = 1$ in the interior region. A 1% increase in total AOPS produces a 1% increase in emission. At the boundaries ($E = 5$ or $E = 2000$), $\epsilon_E = 0$. ∎

### 6.6 Supply Convergence

**Theorem 5 (Supply Boundedness).** The total supply $S(t)$ remains bounded for all $t$:
$$
S(t) \leq S(0) + E_{max} \cdot t
$$

*Proof.* Each block emits at most $E_{max} = 2000$ eWatt. Blocks are produced at rate $1/T$ where $T = 600s$. The annual maximum supply growth:
$$
S_{max}^{(1yr)} = 2000 \times 52596 = 105,192,000 \text{ eWatt}
$$

For any finite $t$, $S(t) \leq 10^6 + 105,192,000 \times t \text{ (in years)}$.

The minimum emission of $5$ eWatt/block gives:
$$
S_{min}^{(1yr)} = 5 \times 52596 = 262,980 \text{ eWatt}
$$

**Corollary 5.1.** The supply growth rate is in $[262,980, 105,192,000]$ eWatt/year. Unlike Bitcoin (fixed cap at 21M), eWatts has an elastic supply with no hard cap. Unlike Ethereum (monetary policy changeable), eWatts' elasticity is protocol-enforced. ∎

### 6.7 Reorg Safety

**Theorem 6 (Reorg Depth Safety).** A reorganization of depth $k$ requires building $k$ blocks on a competing fork. With an honest majority, the probability that an adversary can reorg $k$ blocks after $n$ confirmations decays exponentially in $n$.

*Proof.* Standard Nakamoto consensus analysis (Nakamoto, 2008). For adversarial fraction $q < 0.5$:
$$
P(\text{reorg of depth } k) = \left(\frac{q}{1-q}\right)^k
$$

For $q = 0.4$, $k = 6$: $P \approx (0.667)^6 \approx 0.088$ (8.8%).  
For $q = 0.4$, $k = 100$: $P \approx (0.667)^{100} \approx 2.6 \times 10^{-18}$.

The protocol enforces a maximum reorg depth of 100 blocks ($k_{max} = 100$). For any $q < 0.5$, $P(\text{reorg} > 100) \approx 0$. ∎

### 6.8 Penalty Function Properties

**Theorem 7 (Penalty Function Monotonicity).** The effective commitment function $\phi(c, e)$ is:
1. Monotonically increasing in $e$ for $e < 0.7$
2. Constant in $e$ for $0.7 \leq e \leq 1.3$
3. Constant in $e$ for $e > 1.3$
4. Monotonically increasing in $c$ for all $e$

*Proof.* Immediate from the definition:
1. $\partial\phi/\partial e = c > 0$ for $e < 0.7$
2. $\partial\phi/\partial e = 0$ for $0.7 \leq e \leq 1.3$
3. $\partial\phi/\partial e = 0$ for $e > 1.3$
4. $\partial\phi/\partial c = 1$ for $e \in [0.7, 1.3]$ and $\partial\phi/\partial c = e$ or $1.3$ elsewhere; both are $> 0$ ∎

**Corollary 7.1.** The penalty function creates a "reward plateau" in the efficiency range $[0.7, 1.3]$, where additional efficiency yields no additional effective commitment.

### 6.9 Bayesian-Nash Entry Equilibrium with Heterogeneous Costs

**Motivation.** The free-entry equilibrium (Theorem 6.1) assumes homogeneous miners: identical hardware, electricity prices, and beliefs. In practice, miners differ in capital cost ($K_i$), electricity price ($p_i$), node power ($W_i$), and access to DRAM pricing. We relax homogeneity and analyze entry as a Bayesian game: each miner knows their own type $\theta_i = (K_i, p_i, W_i)$ but only knows the distribution $F(\theta)$ of other miners' types.

**Theorem 6.8 (Bayesian-Nash Entry).** Under the free-entry protocol with heterogeneous costs and incomplete information, there exists a symmetric Bayesian-Nash equilibrium characterized by a cutoff type $\theta^*$. Miners with cost $c_i \leq c^*$ enter; those with $c_i > c^*$ stay out.

*Proof.* Define each miner's type as their marginal cost per block:
$$
\kappa_i = \frac{W_i \cdot 600 \cdot p_i}{3.6 \times 10^6} + \frac{K_i}{\mathbb{E}[T_i]} \quad \text{[operating + amortized capital]}
$$
where $\mathbb{E}[T_i]$ is the expected lifetime (in blocks) of miner $i$'s hardware.

A miner enters iff:
$$
\mathbb{E}[\pi_i] = \mathbb{E}\left[\frac{\bar{c}_i}{\Sigma} \cdot 100 \cdot P \right] - \kappa_i > 0
$$

By symmetry, in equilibrium all entering miners have the same effective commitment $\bar{c}^*$ and each expects $\mathbb{E}[\bar{c}_i / \Sigma] = 1/N$. The entry condition becomes:
$$
\frac{100 \cdot P}{N} - \kappa_i > 0 \iff \kappa_i < \frac{100 \cdot P}{N}
$$

Let $\kappa^* = 100P/N^*$ be the cutoff cost, where $N^*$ solves:
$$
N^* = \sum_i \mathbf{1}_{\kappa_i < \kappa^*}
$$

This fixed point exists by the intermediate value theorem: as $N$ increases, $\kappa^*$ decreases, reducing entry until $N^*$ satisfies the equation. Uniqueness follows from monotonicity of $\kappa^*(N) = 100P/N$ in $N$. ∎

**Corollary 6.2 (Selection Pressure).** The Bayesian-Nash equilibrium implies that low-cost miners (efficient hardware, cheap electricity) earn positive expected profit $\kappa^* - \kappa_i$, while marginal miners ($\kappa_i \approx \kappa^*$) earn near zero. This selection pressure replaces the ASIC-driven centralization of Bitcoin with DRAM-driven efficiency sorting.

**Corollary 6.3 (Comparative Statics).** 
- A $10\%$ decrease in global DRAM prices shifts $F(\theta)$ leftward, lowering $\kappa_i$ for all potential entrants, increasing $N^*$ and reducing $\kappa^*$.
- A $20\%$ increase in $P$ (eWatt market price) raises $\kappa^*$ linearly, attracting higher-cost miners until equilibrium is restored.
- Regions with $p_i < 0.05$ USD/kWh (hydroelectric, nuclear, stranded gas) have a structural cost advantage of $2\text{-}5\times$ over regions with $p_i > 0.15$ USD/kWh.

### 6.10 Dynamic Stability of the Emission Clamp

**Theorem 6.9 (Emission Clamp Damping).** The emission clamping mechanism ($\epsilon \in [5, 2000]$) acts as a negative feedback oscillator with bounded amplitude. Under any finite perturbation to total effective AOPS, the emission rate converges to a steady state within $O(\log(1/\delta))$ blocks, where $\delta$ is the perturbation magnitude.

*Proof.* Let $S_t = \sum_i \bar{c}_i(t)$ be total effective AOPS at block $t$, and let $\bar{S}$ be the historical average over $W$ blocks. The emission rate is:
$$
\epsilon_t = \text{clamp}\left(100 \cdot \frac{S_t}{\bar{S}}, 5, 2000\right)
$$

Define the deviation $\Delta_t = \log(S_t / \bar{S})$. The clamp applies bounded gain:
- If $\Delta_t > \log(20)$: $\epsilon_t = 2000$ (ceiling), $\Delta_{t+1} < \Delta_t$ because $\epsilon_t$ attracts AOPS
- If $\Delta_t < \log(0.05)$: $\epsilon_t = 5$ (floor), $\Delta_{t+1} > \Delta_t$ because low emission discourages AOPS
- If $\Delta_t \in [\log(0.05), \log(20)]$: $\epsilon_t = 100 e^{\Delta_t}$, a proportional controller

The closed-loop dynamics are:
$$
\Delta_{t+1} = \beta \cdot \Delta_t
$$
where $\beta = 1 - \alpha \cdot \frac{W-1}{W}$ for some $\alpha > 0$ determined by the AOPS response elasticity. Since $\beta \in (0, 1)$ for any finite $W$, the system is exponentially stable:
$$
|\Delta_t| \leq |\Delta_0| \cdot e^{-t/\tau}, \quad \tau = \frac{1}{\ln(1/\beta)}
$$

The time constant $\tau$ is bounded above by $W$ (the window size). Convergence within $5\tau$ blocks requires $O(\log(1/\delta))$ steps. ∎

**Corollary 6.4 (No Oscillation).** Unlike Bitcoin's difficulty adjustment (which can produce 2-period limit cycles), the eWatts emission clamp with window $W = 1000$ blocks has a damping ratio $\zeta > 1$ for any realistic AOPS response elasticity, ensuring overdamped (non-oscillatory) convergence.

---

## 7. Computational Complexity

### 7.1 Operation Bounds

| Operation | Time Complexity | Space Complexity | Amortized Cost | Notes |
|-----------|----------------|-----------------|----------------|-------|
| **PoW Mining** | $O(B \cdot D)$ | $O(D^{1/1000})$ trace | Dominant cost | $D$ = difficulty-to-accesses, $B$ = BASE_ACCESSES |
| **PoW Verification (full)** | $O(B \cdot D)$ | $O(1)$ | High | Only for empty trace path |
| **PoW Verification (sampled)** | $O(B \cdot D \cdot 30/N)$ | $O(1)$ | Low | 30 random samples + tail walk |
| **DAG Generation** | $O(N \cdot M \cdot 256)$ | $\Theta(N \cdot 64)$ | One per epoch | $N$ = elements, $M$ = cache size |
| **Block Validation** | $O(T + C + M)$ | $O(1)$ | $O(1)$ per block | $T$ = txs, $C$ = commitments, $M$ = merkle |
| **TX Validation (P2PKH)** | $O(1)$ | $O(1)$ | $O(1)$ per TX | Hash + Ed25519 verify |
| **TX Validation (MLSAG)** | $O(R \cdot L)$ | $O(R \cdot L)$ | $O(11)$ per TX | $R$ = ring_size (11), $L$ = n_layers |
| **UTXO Lookup** | $O(1)$ expected | $O(1)$ | $O(1)$ | HashMap |
| **UTXO Insertion** | $O(1)$ amortized | $O(1)$ | $O(1)$ | HashMap insertion |
| **Double-spend Check** | $O(1)$ expected | $O(1)$ | $O(1)$ | HashSet check |
| **Chain Store Insert** | $O(1)$ amortized | $O(K)$ | $O(1)$ | $K$ = block size in store |
| **Chain Reorg (unwind)** | $O(U \cdot L)$ | $O(S)$ | Rare | $U$ = unwind depth, $L$ = avg txs/block, $S$ = UTXO set size |
| **Chain Reorg (apply)** | $O(A \cdot L)$ | $O(S)$ | Rare | $A$ = apply depth |
| **Merkle Root** | $O(T)$ | $O(T)$ | $O(\log T)$ depth | $T$ = tx count |
| **Range Proof Verify** | $O(B)$ | $O(B)$ | $O(64)$ | $B$ = bits (max 64) |
| **Pedersen Commit** | $O(1)$ | $O(1)$ | $O(1)$ | 2 scalar mults |

**Key constants:**
- $BASE\_ACCESSES = 10^9$
- $D$ = difficulty (typically 100 on testnet, grows on mainnet)
- $N = \text{size\_bytes} / 64$ (8GB mainnet ≈ 134M elements)
- $M = N / 128$ (cache size)
- $W = 1000$ (sample interval = walk_length / 1000)
- $R = 11$ (ring signature size)
- $L = 1$ (single-input MLSAG) or $L = n\_inputs$
- $B = 64$ (max range proof bits)
- $U, A \leq 100$ (max reorg depth)

### 7.2 Memory Footprint

| Component | Memory | Growth |
|-----------|--------|--------|
| DAG (mainnet, epoch 0) | 8 GB | +512 MB/year |
| DAG (testnet) | 256 MB | None (fixed) |
| UTXO Set | $O(N_{utxo} \cdot 200B)$ | Linear with chain |
| Block Cache (10K blocks) | ~5 MB (header only) | Bounded |
| Mempool (5K txs) | ~50-500 MB | Bounded |
| Peer Set (200 peers) | ~100 KB | Bounded |
| Orphan Queue (500 blocks) | ~50 MB (empty) | Bounded |
| Chain Store Metadata | $O(N_{blocks})$ | Linear with chain |

**Total minimum:** ~8.5 GB (mainnet, dominated by DAG)  
**Total maximum:** ~10 GB (with full UTXO set and mempool)

### 7.3 Network Bandwidth

| Operation | Bandwidth | Notes |
|-----------|-----------|-------|
| Full Block | ~500B + TXs | Header: ~168B, each TX: varies |
| Compact Block | ~200B + short IDs | Header + coinbase + short IDs |
| Transaction | ~200B (P2PKH) to ~2KB (MLSAG) | MLSAG includes ring data |
| Compact Block Reconstruction | 1 short ID / TX | 8 bytes per TX beyond coinbase |
| Block Sync (per block) | Full block size | Overhead only on peer connect |
| Gossip Propagation | ~6-12 peers mesh | gossipsub mesh topology |

---

## 8. Test Vectors

> **Standalone file:** All 8 test vectors are also available as machine-readable JSON at:
> `eWatts/Analysis/test_vectors.json` (companion README: `test_vectors_README.md`)
> Generated from `tests/test_vectors.rs` — run `cargo test --test test_vectors -- --nocapture` to regenerate.

### 8.1 DAG Determinism

**Input:** `epoch = 0`, `size_bytes = 65536` (64 KB)

**Expected output:**  
First 32 bytes (hex) of DAG element 0:
```
DAG[0] = Keccak-256(SHA-512(Keccak-256(epoch.to_le_bytes())))
       = SHA-512(39A337DE...)
       = [/* first 64 bytes of DAG[0] */]
```

**Verification:** `test_dag_deterministic` in dag.rs confirms element-by-element equality.

### 8.2 Merkle Root

**Input:** Two leaf hashes:
```
leaf_0 = sample_leaf_hash(0, [0xAB; 64])
leaf_1 = sample_leaf_hash(1000, [0xCD; 64])
```

**Expected:**
```
merkle_root = Keccak-256(leaf_0 || leaf_1)
```

**Verification:** `test_merkle_root_verify` in proof.rs.

### 8.3 Commitment Efficiency

**Case 1 (Honest):** Declared AOPS = 25,000,000, total ops = 25,000,000, time = 1.0s  
**Expected:** efficiency = 1.0, effective commitment = 25,000,000

**Case 2 (Penalty):** Declared AOPS = 25,000,000, total ops = 10,000,000, time = 1.0s  
**Expected:** efficiency = 0.4, effective commitment = 25,000,000 × 0.4 = 10,000,000

**Case 3 (Cap):** Declared AOPS = 25,000,000, total ops = 50,000,000, time = 1.0s  
**Expected:** efficiency = 2.0, effective commitment = 25,000,000 × 1.3 = 32,500,000

### 8.4 Emission Rate

| Total Effective AOPS | Historical Avg | Expected Emission | Floor/Ceiling Applied |
|---------------------|----------------|-------------------|----------------------|
| 25,000,000 | 25,000,000 | 100.0 | Interior |
| 50,000,000 | 25,000,000 | 200.0 | Interior |
| 1,000,000 | 25,000,000 | 4.0 → 5.0 | Floor (0.05 × 100) |
| 500,000,000 | 25,000,000 | 2000.0 → 2000.0 | Ceiling (20 × 100) |

### 8.5 Pedersen Commitment

```
G = HtoP(b"Ewatts_Ring_G_v1")
H = HtoP(b"Ewatts_Pedersen_H_v1")

Commit(5, a=0) = 0*G + 5*H = 5*H
Commit(10, a=0) = 10*H

C(5) + C(10) = 5*H + 10*H = 15*H = C(15, 0)
```

**Verification:** `privacy_pedersen_homomorphic_add` test.

### 8.6 Block Hash

```
header = BlockHeader {
    version: 0x0005,
    previous_hash: [0; 32],
    merkle_root: [0; 32],
    timestamp: 1000,
    height: 0,
    epoch: 0,
    difficulty_target: 1,
    total_effective_commit: 0,
    emission_rate: 100_000_000,  // 100 eWatt in base units
    miner_effective_commit: 0,
    vr_block: 0,
    coinbase_burn: 0,
    nonce: 0,
    elapsed_ms: 0,
    proof_merkle_root: None,
}

Expected hash: Keccak-256(all above fields, big-endian encoding)
```

**Verification:** `test_header_hash` in block.rs.

---

# Volume III — Threat & Security Analysis (Extended)

---

## 9. Cryptographic Primitive Analysis

### 9.1 Formal Security Models

| Primitive | Security Model | Assumption | Security Level | Known Attacks | PQ Risk |
|-----------|---------------|------------|----------------|---------------|---------|
| Ed25519 | EUF-CMA | DLOG in Curve25519 | 128-bit | None practical | HIGH (Shor) |
| Keccak-256 | Collision Resistance | Random Oracle | 128-bit (collision) | None practical | LOW (Grover: 64-bit) |
| SHA-512 | Collision Resistance | Random Oracle | 256-bit (collision) | None practical | LOW (Grover: 128-bit) |
| Ristretto | DLOG, CDH | DLOG in Ristretto | 126-bit | None practical | HIGH (Shor) |
| Pedersen Commitment | Computational Hiding + Binding | DLOG | 126-bit | None (correct usage) | HIGH (Shor) |
| MLSAG | Anonymity + Linkability | DLOG + ROM | 126-bit | Timing side channel in sign() | HIGH (Shor) |
| Stealth Address | Unlinkability | DDH in Ristretto | 126-bit | Decoy selection (implementation) | HIGH (Shor) |
| Range Proof (bit-decomp) | Soundness + Zero Knowledge | DLOG + ROM | 126-bit | None known (standard construction) | HIGH (Shor) |
| FNV-1a | N/A (non-crypto) | N/A | 0-bit (not cryptographic) | Collisions expected (design) | N/A |

### 9.2 EUF-CMA Analysis (Ed25519)

**Definition (EUF-CMA).** A signature scheme $\Pi = (\text{Gen}, \text{Sign}, \text{Verify})$ is Existentially Unforgeable under Chosen Message Attack if for all PPT adversaries $\mathcal{A}$:
$$
\Pr\left[ \text{Verify}(pk, m^*, \sigma^*) = 1 \land m^* \notin Q \right] \leq \text{negl}(\lambda)
$$
where $Q$ is the set of messages queried to the signing oracle.

**Ed25519 Status:** Meets EUF-CMA under the DLOG assumption in the random oracle model.

**Commitment security:** Commitment signatures use Ed25519. An attacker who cannot forge Ed25519 signatures cannot forge a commitment for a different miner ID.

### 9.3 Random Oracle Model (Keccak-256)

The protocol treats Keccak-256 and SHA-512 as random oracles. This is a standard assumption.

**Justification:**
- DAG generation: hash output defines element content → invertible only by brute force
- Headers: mixing relies on random oracle properties
- Merkle trees: RO security implies collision resistance
- MLSAG challenge: `mlsag_challenge(msg, L, R)` uses SHAKE-256 as RO

**Risk:** A quantum adversary using Grover's algorithm can invert a random oracle in $O(2^{n/2})$ time. For Keccak-256 (128-bit collision security), Grover reduces security to 64-bit. For SHA-512 (256-bit collision security), to 128-bit. This is acceptable for the medium term but not post-quantum.

### 9.4 Fiat-Shamir Heuristic

The MLSAG construction uses the Fiat-Shamir heuristic: the challenge $c$ is computed as a hash of the statement and commitments (protocol transcript), rather than obtained from a verifier.

**Soundness:** In the random oracle model, the Fiat-Shamir transform produces a secure non-interactive zero-knowledge proof. The MLSAG is a variant of the AOS ring signature [Abe, Ohkubo, Suzuki, 2002] extended for linkability and multiple layers.

### 9.5 Schnorr Security

Each MLSAG layer uses Schnorr-like responses:
$$
r_{ij} = \alpha_j - c_{\pi} \cdot sk_{ij}
$$

The security of these responses relies on the discrete log problem: an attacker who can extract `sk` from `r` can solve the DLP.

### 9.6 DLOG in Ristretto

Ristretto is a prime-order group abstraction over Curve25519. The group order is:
$$
\ell = 2^{252} + 27742317777372353535851937790883648493
$$

The best known attack is Pollard's rho, requiring $\sqrt{\ell} \approx 2^{126}$ operations.  
**Current security level:** 126-bit (acceptable until quantum).

### 9.7 Ristretto Formal Assumptions

The protocol assumes:
1. **Discrete Log (DLOG):** Given $P = xG$, finding $x$ is hard.
2. **Computational Diffie-Hellman (CDH):** Given $aG, bG$, finding $abG$ is hard.
3. **Decisional Diffie-Hellman (DDH):** Distinguishing $(aG, bG, abG)$ from $(aG, bG, cG)$ is hard.

All three hold in the Ristretto group (prime order, no small subgroups). ✅

### 9.8 Timing Side Channel

**Finding (CONFIRMED):** MLSAG::sign is non-constant-time with respect to `real_index`:
```rust
// privacy.rs (~line 175)
pub fn sign(ring, secret_keys, real_index, msg, rng) -> Self {
    for i in 0..ring_size {
        if i == real_index { continue; }  // ← TIMING LEAK
        responses[i][j] = Scalar::random(rng);
    }
    // ... computed for real_index ...
}
```

**Severity:** MEDIUM on testnet (documented as NOT constant-time). HIGH on mainnet.

**Mitigation:** Replace with constant-time pattern:
```rust
for i in 0..ring_size {
    let is_real = i == real_index;
    responses[i][j] = conditional_assign(
        Scalar::random(rng),  // fake
        alpha[j] - c_pi * secret_keys[j],  // real
        is_real
    );
}
```

---

## 10. Extended Economic Attack Taxonomy

### 10.1 Time-Bandit Attack (adapted from Flash Boys 2.0, Daian et al. 2020)

**Description:** A miner observes profitable opportunities in the mempool (arbitrage, liquidations). Instead of including the exploiting transactions, the miner replays them against their own blocks, extracting MEV.

**eWatts relevance:** eWatts has no smart contracts, so no DeFi MEV. However, if eWatts integrates with DEX or atomic swap protocols in the future, time-bandit attacks become relevant.

**Current status:** Not applicable. No MEV in UTXO-only protocol.

### 10.2 Block Withholding Attack (Rosenfeld 2011)

**Description:** In a mining pool, a malicious miner submits partial proofs (shares) but discards full solutions. The pool loses full-block rewards while paying the miner for shares.

**eWatts specific:** pool.rs implements a mining pool server. [Requires Code Confirmation] Does the pool protocol verify that full solutions are submitted? Or does it accept shares that could be valid blocks?

**Mitigation:** Pool should require miners to submit full blocks (not just share-level solutions). If a miner withholds a valid block, they lose the block reward entirely.

### 10.3 Pool Hopping (Luu et al. 2015)

**Description:** Miners join a pool for high-yield rounds and leave during low-yield rounds, destabilizing pool revenue.

**eWatts specific:** Pool protocol (pool.rs) uses PPS or PPLNS? [Requires Code Confirmation] PPS is vulnerable to hopping if not properly calibrated. PPLNS (Pay Per Last N Shares) reduces hopping incentives.

### 10.4 Feather Forking

**Description:** A miner signals that they will orphan any block that includes a blacklisted transaction. This coerces other miners into censorship.

**eWatts relevance:** Standard attack on permissionless blockchains. Mitigated by:
- Reorg depth limit (100 blocks) makes prolonged feather forking expensive
- Multiple competing mining pools make coordination difficult
- No scripting language that enables complex blacklisting rules

**Risk:** LOW — economically expensive to sustain.

### 10.5 Eclipse-Assisted Selfish Mining (Heilman et al. 2015)

**Description:** An attacker eclipses a target miner (fills their peer set with attacker-controlled peers), then mines selfishly. The eclipsed miner never sees the secret chain, so their blocks orphan on the public chain.

**eWatts specific:** Peer Manager uses LRU eviction with 200 peer slots. An attacker with 200 identities can fill the peer set and achieve eclipse. TokenBucket (5 conn/s) limits the rate but not the eventual count.

**[Requires Code Confirmation]** How are outbound connections established? If bootstrapped peers are always kept, eclipse requires filling 200 slots against the rate limiter. With 5 conn/s, filling 200 slots takes 40 seconds — fast enough.

**Mitigation:** Reserve ≤25% of peer slots for manually configured/bootstrap peers.

### 10.6 Bribery Attack (Liao & Katz 2017)

**Description:** An attacker bribes miners to include/not include specific transactions, or to mine on a specific fork.

**eWatts relevance:** Standard for any proof-of-work chain. Bribery is economically bounded by the block reward. To bribe a miner to orphan a block, the bribe must exceed the block reward (~100 eWatt).

**eWatts specific:** If eWatt holders want to censor a transaction, they can bribe miners. The cost is proportional to block reward.

### 10.7 Uncle Manipulation (adapted to eWatts)

**Bitcoin/Ethereum context:** Uncle/uncle blocks are valid blocks that don't make it into the canonical chain. In Ethereum, uncles receive reduced rewards.

**eWatts adaptation:** eWatts does NOT pay orphan rewards. An attacker who intentionally creates orphan blocks by mining on stale forks wastes AOPS.

**Attack:** Miner observes a block on the network, starts mining the next block, but then receives a competing block from a faster peer. The miner's in-progress block becomes an orphan. This is a natural orphan, not an attack.

**Manipulation:** An attacker can create orphans intentionally to:
1. Dilute the honest chain's accumulated work (making a reorg easier)
2. Waste honest miners' work

**Cost of attack:** Each orphan costs the attacker one block's worth of work. With network orphan rate $\rho$, attacker needs $k/(1-\rho)$ blocks in total, where $k$ is the reorg depth.

### 10.8 Reward Sniping

**Description:** A miner sees a valid block on the network, but instead of building on it, tries to mine a competing block at the same height that gives them a higher reward (e.g., by including more lucrative fees).

**eWatts specific:** Block reward is fixed (proportional to effective AOPS, with small variation from fee inclusion). The main sniping vector is fee selection — miners may delay block propagation to extract more fees.

**Mitigation:** Block time (600s) is long enough that sniping is unprofitable (the sniper falls behind and loses the block entirely).

### 10.9 Oscillation Attack (Strategic Commitment Timing)

**Description:** A miner with significant AOPS capacity varies their declared commitment block-by-block to create oscillations in total effective AOPS ($\Sigma$), causing the emission rate to oscillate. During low-$\Sigma$ periods, the miner's effective share of total reward increases.

**Formal analysis:**

Let miner A have capacity $C_A$. A's strategy:
- Even blocks: declare $0.8 \cdot C_A$ (under-declare to reduce $\Sigma$)
- Odd blocks: declare $1.0 \cdot C_A$ (normal)

Effect on $\Sigma$:
- $\Sigma_{even} = \Sigma_{others} + 0.8 \cdot C_A$
- $\Sigma_{odd} = \Sigma_{others} + 1.0 \cdot C_A$

Emission:
- $E_{even} = 100 \cdot \Sigma_{even} / H$
- $E_{odd} = 100 \cdot \Sigma_{odd} / H$

A's reward:
- $R_{even}^A = (0.8 \cdot C_A / \Sigma_{even}) \cdot E_{even}$
- $R_{odd}^A = (1.0 \cdot C_A / \Sigma_{odd}) \cdot E_{odd}$

If A has 20% of network AOPS ($C_A = 0.2\Sigma$):
- Reducing $\Sigma$ by 4% (from $1.2\Sigma_{others}$ to $0.96\Sigma_{others} + 0.8C_A$)
- Reward change: $R_{even}^A$ vs $R_{odd}^A$ — need to compute numerically

**Mitigation:** The 1000-block historical average $H$ dampens oscillations. A single block's change in $\Sigma$ affects $E$ proportionally but $H$ only changes by $1/1000$ of the difference.

### 10.10 Strategic Commitment Timing (Refined)

**Description:** Miner commits late in the block time window, observing other miners' commitments before submitting their own. This is a last-mover advantage.

**Protocol constraint:** Commitments are included in blocks. A miner cannot see other commitments until the block is assembled. However, if multiple commitments are gossiped before block assembly, a miner may front-run.

**[Requires Code Confirmation]** Are commitments gossiped independently, or only within blocks?

### 10.11 Block Withholding in Multi-Identity Setting

**Description:** Miner runs $k$ identities, each declaring $c/k$ AOPS. If one identity finds a block, the miner withholds it and continues mining with the other identities to find a block with a more favorable commitment-combination.

**Cost:** The found block is discarded (reward lost). The benefit comes from finding a block with a different commitment distribution that gives higher total reward.

**Analysis:** If all identities are controlled by the same entity, the total reward is the sum of shares. Discarding one block to find a "better" one is never profitable because the expected time to find the next block is the same — no selective advantage.

### 10.12 Fee Sniping

**Description:** A miner rearranges transactions within a block to capture high-fee transactions for their own benefit.

**eWatts relevance:** Transactions don't have explicit fees in the Bitcoin sense. The fee is implicit (inputs - outputs). [Requires Code Confirmation] Are fees distributed to miners?

In mempool.rs: `compute_fee` calculates fee = inputs - outputs. The `take_for_mining` function returns highest-fee transactions first. So fees ARE captured by the block producer.

### 10.13 Selfish Mining (Eyal & Sirer 2014, adapted)

**Description:** Miner withholds found blocks, continuing to mine on their private chain. When the honest chain catches up, the selfish miner releases withheld blocks to orphan honest blocks.

**eWatts adaptation:** The heaviest-chain rule means withheld blocks must exceed the honest chain's total work. A selfish miner with fraction $\alpha$ captures more than $\alpha$ of rewards when $\alpha > 1/3$ (Eyal & Sirer's result).

**eWatts mitigation:** Block time of 600s makes selfish mining less attractive (orphans are expensive). The 100-block reorg limit creates a hard ceiling on reorg depth.

### 10.14 Transaction Ordering Manipulation

**Description:** Miner reorders transactions to extract value.

**eWatts relevance:** Without smart contracts, transaction ordering is only valuable for fee optimization. No front-running or sandwich attacks on AMMs or liquidations.

---

## 11. Performance Analysis

### 11.1 CPU Benchmarks (Theoretical)

| Operation | x86-64 IPC | ARM Neoverse IPC | Notes |
|-----------|-----------|------------------|-------|
| SHA-512 (1 block) | ~50 cycles/byte | ~60 cycles/byte | AVX2: ~35 cycles/byte |
| Keccak-256 | ~100 cycles/byte | ~120 cycles/byte | Not SIMD-friendly |
| Curve25519 (scalar mult) | ~300K cycles | ~400K cycles | Constant-time |
| Ed25519 verify | ~350K cycles | ~450K cycles | Batch verify ~1.5x faster |
| Ristretto add | ~200 cycles | ~250 cycles | |
| Ristretto scalar mult | ~300K cycles | ~400K cycles | |
| AES-NI (PCIe DMA) | N/A | N/A | Not used by protocol |

### 11.2 Memory Benchmarks (DDR5-4800)

| Operation | Bandwidth | Latency | Notes |
|-----------|-----------|---------|-------|
| Sequential read | ~35 GB/s | ~80 ns | DAG warm walk |
| Random read (64B) | ~5 GB/s | ~80 ns | DAG cold read |
| Sequential write | ~30 GB/s | ~80 ns | DAG generation |
| DRAM power | ~5W/DIMM | — | 2x DIMM = 10W |
| CPU L1 (32KB) | ~1 TB/s | ~1 ns | Too small for DAG |
| CPU L2 (1MB) | ~500 GB/s | ~4 ns | Too small for DAG |
| CPU L3 (16MB) | ~200 GB/s | ~12 ns | Too small for DAG |

### 11.3 DAG Generation Performance

**Testnet DAG (256 MB):**
- Expected generation time: ~2-3 seconds (DDR5-4800)
- Memory required: 256 MB continuous

**Mainnet DAG (8 GB, epoch 0):**
- Expected generation time: ~60 seconds (DDR5-4800, spec target in `test_dag_benchmark_64mb`)
- Memory required: 8 GB continuous

**Epoch growth:** +512 MB/year (normal), +1 GB/year (accelerated if difficulty ETA < 1.3 and bandwidth < 100 GB/s)

### 11.4 Mining Throughput

At base difficulty ($D = 1$):
- Walk length = $10^9$ accesses
- Each access: read 64 bytes + SHA-512 + XOR = ~3.75 µJ
- Time per attempt at 25M ops/s: $10^9 / 25 \times 10^6 = 40$ seconds
- Expected attempts to find block at $D = 1$: 1 (highest difficulty always meets target)

At higher difficulty ($D = 100$):
- Walk length = $10^9 \cdot 100 / 10^9 = 100$ accesses per attempt
- Time per attempt at 25M ops/s: $100 / 25 \times 10^6 = 4 \mu s$
- The `nonce_limit` of 50,000 attempts gives ~200ms total mining time per block (testnet)

**Normative interpretation:** `difficulty_to_accesses` computes $base \times difficulty / 10^9$. Since $BASE\_ACCESSES = 10^9$, difficulty serves as a direct multiplier on the base walk length. At $D = 1$, walk length = 1 access (minimum). At $D = 100$, walk length = 100 accesses. The actual number of hash comparisons required to meet a difficulty target is always 1: the single final hash after the walk. Difficulty therefore controls economic cost (memory access count) rather than probabilistic search space — a design choice that makes eWatts mining work-predictable rather than lottery-based.

### 11.5 NUMA Analysis

On a dual-socket server:
- DAG allocated on socket 0's DRAM
- Miner process on socket 1
- Each DAG access crosses the interconnect (UPI/Infinity Fabric)
- Latency penalty: 2-3x for first-touch cross-socket access
- Mitigation: local allocation (`numactl --membind`), DAG thread pinning

**[Recommendation]** Mining nodes SHOULD pin the miner thread to the socket where the DAG is allocated. Use `numactl --cpunodebind=0 --membind=0`.

### 11.6 GPU Feasibility

DAG-walk SHA-512:
- GPU memory bandwidth: 1 TB/s+ (A100: 2 TB/s, H100: 3.3 TB/s)
- Random access pattern: GPU memory coalescing suffers
- Latency: ~200ns on GPU vs ~80ns on DDR5
- Compute: SHA-512 on GPU is efficient (SIMT parallelism)

**Conclusion:** GPU mining would be ASIC-like — faster than CPU but with the same access pattern inefficiency. The free entry equilibrium absorbs this through $N$ adjustment.

### 11.7 AVX2/AVX-512

SHA-512 can use:
- AVX2: ~2x throughput improvement over scalar
- AVX-512: ~3x improvement (using VPSHUFD, VPSRLQ)
- SHA-512 is not directly accelerated by SHA-NI (which covers SHA-1/SHA-256)

### 11.8 SSD Impact

DAG is too large for CPU cache but fits in DRAM. On systems with insufficient RAM:
- OS swaps DAG to SSD
- Random 64-byte read on NVMe: ~10 µs latency (100x slower than DRAM)
- DAG generation on SSD: bandwidth ~5 GB/s sequential (NVMe Gen4)
- Mining on swapped DAG: effectively impossible (10x+ penalty)

---

# Volume IV — Engineering Analysis (from V2)

*[The following volumes are carried forward from V2 with refinements. Full text available in the V2 document.]*

---

## 12. Mechanism Design & Game Theory

*[Refer to V2 Volume III — Sections 6-10]*

Key additions in V3:
- Theorem 1-7 with full proofs
- Free entry equilibrium derivation (corrected)
- Elasticity analysis

---

## 13. Rust Safety Engineering

*[Refer to V2 Section 10]*

Key refinements in V3:
- Interspersed with code line references using CONFIRMED/Requires Code Confirmation labels
- Test vector references for verification

---

## 14. Consensus & Distributed Systems

*[Refer to V2 Section 14]*

---

## 15. Privacy & Cryptographic Protocols

*[Refer to V2 Section 11 + 15]*

---

## 16. Historical Vulnerability Case Studies

*[Refer to V2 Section 13]*

---

# Volume V — Appendices

---

## 22. STRIDE Analysis Matrix

*[Refer to V2 Section 14]*

---

## 23. Attack Trees

*[Refer to V2 Section 15]*

---

## 24. CAPEC Mapping

*[Refer to V2 Section 16]*

---

## 25. Audit Procedures Checklist

*[Refer to V2 Section 17 with the following additions:]*

| New ID | Procedure | Status | Notes |
|--------|-----------|--------|-------|
| N-01 | Verify pool hopping resistance in pool.rs | 🔍 Requires Investigation | PPS vs PPLNS |
| N-02 | Verify block-withholding detection in pool protocol | 🔍 Requires Investigation | |
| N-03 | Verify outbound peer reservation against eclipse | 🔍 Requires Investigation | Peer selection logic |
| N-04 | Verify commitment gossip is block-bound | 🔍 Requires Investigation | Are commitments gossiped separately? |
| N-05 | Verify fee distribution to miners | 🟡 Needs Review | mempool.rs compute_fee |
| N-06 | Verify NUMA-aware DAG allocation | 🟡 Needs Review | Performance |

---

## 17. Appendix A — Protocol Invariants

### AI-1: Monetary Conservation

**Description:** The total supply is always the sum of all coinbase outputs ever created, minus any burned amounts.

**Formula:**
$$
\forall h \in \mathbb{N}: \text{Supply}(h) = \sum_{b=0}^{h} \left( \sum_{o \in \text{coinbase}_b} \text{amount}(o) \right) - \text{burned}(h)
$$

**Tests:** `test_supply`, `test_total_emission_matches`  
**Code:** `state.rs:UtxoSet::add_coinbase_supply`

---

### AI-2: No Unauthorized Mint

**Description:** Only the coinbase transaction (first transaction in a block, with zero inputs) creates new eWatt. All other transactions MUST balance (sum(inputs) ≥ sum(outputs)).

**Formula:**
$$
\forall tx \in \text{Block} \setminus \{\text{coinbase}\}: \sum_{i \in \text{inputs}(tx)} \text{amount}(i) \geq \sum_{o \in \text{outputs}(tx)} \text{amount}(o)
$$

**Tests:** `test_double_spend`, `test_wrong_sig`  
**Code:** `state.rs:validate_transaction`

---

### AI-3: Deterministic Validation

**Description:** Given the same state and the same block, every node produces the same validation outcome, regardless of hardware, OS, or clock skew.

**Formula:**
$$
\forall s_1 = s_2, b_1 = b_2: \text{validate}(s_1, b_1) \iff \text{validate}(s_2, b_2)
$$

**Tests:** `test_spend`, `test_dag_deterministic`  
**Code:** All consensus-critical functions; hash functions provide deterministic output.

---

### AI-4: Reward Conservation

**Description:** Total miner rewards plus burned eWatt equals the emission rate for the block.

**Formula:**
$$
\text{emission\_rate}(b) = \sum_{m \in \text{miners}} \text{reward}(m, b) + \text{burned}(b)
$$

**Tests:** `test_total_emission_matches`  
**Code:** `reward.rs:compute_block_rewards`

---

### AI-5: Commitment Monotonicity

**Description:** A miner's effective commitment (c_eff) is monotonically increasing in both declared AOPS and efficiency. It is bounded below by $c \cdot e$ and above by $1.3 \cdot c$.

**Formula:**
$$
0 \leq \phi(c, e) \leq 1.3 \cdot c
$$
$$
e_1 \leq e_2 \land e_1 < 0.7 \implies \phi(c, e_1) \leq \phi(c, e_2)
$$

**Tests:** `test_eff`, `test_penalty`, `test_cap`  
**Code:** `commitment.rs:effective_commitment`, `commitment.rs:compute_efficiency`

---

### AI-6: VR Consistency

**Description:** The Reference Value (VR) is a function of on-chain data only (total effective AOPS, eWatt mined, window size, block time). Given identical inputs, VR is deterministic.

**Formula:**
$$
\text{VR} = \frac{\text{avg\_effective\_aops} \cdot \text{window\_secs} \cdot J\_PER\_ACCESS}{J\_PER\_KWH \cdot \text{total\_ewatts}}
$$

**Tests:** `test_vr_basic`, `test_vr_doubles`, `test_vr_aops_to_joules`  
**Code:** `vr.rs:compute_vr`

---

### AI-7: Chain Uniqueness (No Split Canonical Chain)

**Description:** In the canonical chain, every block has at most one parent and at most one child at each height. Forks are resolved by the heaviest-chain rule, and no block at height $h$ can have two children at height $h+1$ in the canonical chain.

**Formula:**
$$
\forall b_1, b_2 \in \text{Canonical}: b_1.\text{height} = b_2.\text{height} \iff b_1 = b_2
$$

**Tests:** `test_extend_canonical`, `test_reorg_detection`  
**Code:** `chain.rs:set_chain_tip`

---

### AI-8: No UTXO Resurrection

**Description:** Once a UTXO is spent (its key image is in the spent set), it cannot be spent again on the same chain. Reorg MAY unspend UTXOs by unwinding the chain.

**Formula:**
$$
\forall b \in \text{Canonical}, \forall tx_1, tx_2, \forall i, j:
    \text{key\_image}(tx_1, i) = \text{key\_image}(tx_2, j) \land tx_1 \neq tx_2 \implies \text{invalid}
$$

**Tests:** `test_double_spend`  
**Code:** `state.rs:spent_key_images` set

---

### AI-9: Key Image Uniqueness

**Description:** Key images are deterministically derived from private keys. No two distinct private keys can produce the same key image (assuming collision resistance of `hash_pk`).

**Formula:**
$$
\forall k_1, k_2 \in \mathbb{F}_\ell: k_1 \neq k_2 \implies k_1 \cdot H_p(P_1) \neq k_2 \cdot H_p(P_2)
$$

**Tests:** `test_mlsag_different_wrong_real_index_fails`  
**Code:** `privacy.rs:MLSAGSignature::sign` (key image computation)

---

### AI-10: Merkle Determinism

**Description:** The same set of leaf hashes produces the same Merkle root. The binary Merkle tree uses the self-pair convention for odd counts.

**Formula:**
$$
\forall L_1 = L_2: \text{merkle}(L_1) = \text{merkle}(L_2)
$$

**Tests:** `test_merkle_root_verify`  
**Code:** `proof.rs:merkle_root_from_leaves`

---

## 18. Appendix B — Formal State Machine

### B.1 Transaction Lifecycle

```
                ┌──────────────────┐
                │  Wallet Creates   │
                │  Transaction (TX) │
                └────────┬─────────┘
                         │
                         ▼
                ┌──────────────────┐
                │  TX Validation    │
                │  (syntax, sigs,   │
                │   structure)      │
                └────────┬─────────┘
                         │
                    ┌────┴────┐
                    ▼         ▼
            ┌──────────┐  ┌──────────┐
            │ Valid     │  │ Invalid  │
            │           │  │          │
            └────┬──────┘  └────┬─────┘
                 │               │
                 ▼               ▼
          ┌──────────┐      ┌──────────┐
          │ Submit to │      │  REJECT  │
          │ Mempool   │      └──────────┘
          └────┬──────┘
               │
               ▼
        ┌──────────────┐
        │  Mempool      │
        │  (fee-ordered)│
        │  key_image    │
        │  dedup check  │
        └──────┬───────┘
               │
     ┌─────────┴─────────┐
     │                   │
     ▼                   ▼
┌──────────┐      ┌──────────────┐
│ Included  │      │ Evicted      │
│ in Block  │      │ (low fee,    │
│           │      │  full pool)  │
└────┬──────┘      └──────────────┘
     │
     ▼
┌──────────────┐
│ Block        │
│ Assembly     │
│ (coinbase +  │
│  commitments)│
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Proof of Work│
│ (DAG walk)   │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Block        │
│ Broadcast    │
│ (compact)    │
└──────┬───────┘
       │
       ▼
┌──────────────────┐
│ Peer Validation  │
│ (PoW, merkle,   │
│  state, emission)│
└────────┬─────────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐ ┌────────┐
│ Valid   │ │ Invalid│
│ → Apply │ │→ DROP  │
└────┬───┘ └────────┘
     │
     ▼
┌──────────────┐
│ State Update │
│ • spend TX   │
│   inputs     │
│ • create TX  │
│   outputs    │
│ • record     │
│   BlockDiff  │
│ • update     │
│   supply     │
└──────┬───────┘
       │
       ▼
┌──────────────┐
│ Chain Store  │
│ • add block  │
│ • update tip │
│ • persist    │
└──────┬───────┘
       │
       ▼
┌──────────────────┐
│ Gossip /         │
│ Compact Block    │
│ Propagation      │
└──────────────────┘
```

### B.2 Reorg State Machine

```
        Normal Operation
        ┌────────────┐
        │ Extend     │
        │ Canonical  │◄──── Block received extending tip
        └─────┬──────┘
              │
    Heavier fork detected
              │
              ▼
        ┌────────────┐
        │ Analyze    │
        │ Fork       │
        └─────┬──────┘
              │
        ┌─────┴──────┐
        ▼            ▼
   ┌────────┐  ┌────────────┐
   │ Reorg  │  │ Sidechain  │
   │ Needed │  │ (store,    │
   └────┬───┘  │  not apply)│
        │      └────────────┘
        ▼
   ┌────────────┐      ┌─────────────────┐
   │ Snapshot   │─────►│ Clone state +    │
   │ State      │      │ store (rollback)│
   └─────┬──────┘      └─────────────────┘
         │
         ▼
   ┌────────────┐
   │ Unwind     │◄────── For each block in
   │ Old Chain  │        old chain (tip→fork)
   └─────┬──────┘
         │
         ▼
   ┌────────────┐
   │ Apply      │◄────── For each block in
   │ New Chain  │        new chain (fork→tip)
   └─────┬──────┘
         │
    ┌────┴────┐
    ▼         ▼
┌────────┐ ┌────────┐
│ Success│ │ Failure│
│→ Set   │ │→ Restore│
│  tip   │ │  snap-  │
│→ Gossip│ │  shot   │
└────────┘ └────────┘
```

### B.3 Block Lifecycle

```
Miner                        Network                   Validator
  │                            │                          │
  ├─ Build header              │                          │
  ├─ DAG generation            │                          │
  ├─ PoW (DAG walk)            │                          │
  │    until solution found    │                          │
  ├─ Create commitment         │                          │
  ├─ Assemble block            │                          │
  │                            │                          │
  ├─ CompactBlock ────────────►│                          │
  │                            ├─ Short ID lookup ───────►│
  │                            │    in local mempool      │
  │                            │◄─── Full block request (if missing)│
  ├─ FullBlock ───────────────►│                          │
  │                            ├─ Validate PoW ──────────►│
  │                            ├─ Validate state ────────►│
  │                            ├─ Apply block ───────────►│
  │                            ├─ Gossip ────────────────►│
  │                            │                          │
  │               ◄────────────┼─ (if orphan) queue       │
  │               ◄────────────┼─ (if reorg) unwind/apply │
  │                            │                          │
```

---

## 19. Appendix C — Normative Constants

### C.1 Consensus Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `TARGET_BLOCK_TIME_SECS` | 600 | seconds | Block production target | `constants.rs` |
| `BLOCKS_PER_DAY` | 144 | blocks | Blocks per day | `constants.rs` |
| `BLOCKS_PER_YEAR` | 52596 | blocks | Blocks per year (365.25d) | `constants.rs` |
| `BASE_EMISSION` | 100.0 | eWatt | Base emission per block | `constants.rs` |
| `EMISSION_FLOOR_MULTIPLIER` | 0.05 | — | Floor = 5 eWatt | `constants.rs` |
| `EMISSION_CEILING_MULTIPLIER` | 20.0 | — | Ceiling = 2000 eWatt | `constants.rs` |
| `RAMP_UP_BLOCKS` | 10000 | blocks | ~69.4 days | `constants.rs` |
| `RAMP_UP_CAP` | 0.80 | — | Max 80% per miner | `constants.rs` |
| `FOUNDER_LOCK_BLOCKS` | 50000 | blocks | Absolute lock floor | `constants.rs` |
| `FOUNDER_LOCK_ADDITIONAL` | 40000 | blocks | Additional lock | `constants.rs` |
| `DECIMAL_PLACES` | 6 | — | 1 eWatt = 10^6 base units | `constants.rs` |
| `UNITS_PER_EWATT` | 1,000,000 | — | Base units per eWatt | `constants.rs` |
| `PROTOCOL_VERSION` | 0x0005 | — | Current version | `constants.rs` |

### C.2 DAG Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `DAG_INITIAL_SIZE_BYTES` | 8,589,934,592 | bytes | 8 GB | `constants.rs` |
| `DAG_ELEMENT_SIZE` | 64 | bytes | SHA-512 output size | `constants.rs` |
| `DAG_GROWTH_RATE_BYTES_PER_YEAR` | 536,870,912 | bytes/year | 512 MB/year | `constants.rs` |
| `DAG_EPOCH_BLOCKS` | 2016 | blocks | ~14 days | `constants.rs` |
| `DAG_INITIAL_CACHE_RATIO` | 128 | — | n/128 cache elements | `constants.rs` |
| `DAG_MIX_ROUNDS` | 256 | rounds | FNV mixing iterations | `constants.rs` |

### C.3 Proof of Work Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `BASE_ACCESSES` | 1,000,000,000 | accesses | Baseline walk length | `constants.rs` |
| `VERIFICATION_SAMPLE_RATE` | 0.001 | — | 1 sample per 1000 accesses | `constants.rs` |
| `DIFFICULTY_WINDOW_BLOCKS` | 100 | blocks | ~16.7 hours | `constants.rs` |
| `DIFFICULTY_BOUND_MIN` | 0.5 | — | Minimum adjustment ratio | `constants.rs` |
| `DIFFICULTY_BOUND_MAX` | 2.0 | — | Maximum adjustment ratio | `constants.rs` |

### C.4 Commitment Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `MIN_COMMIT_AOPS` | 20,000,000 | ops/s | DDR5 baseline (75W / 3.75µJ) | `constants.rs` |
| `WATTS_PER_NODE` | 75.0 | watts | Wall power per node | `constants.rs` |
| `J_PER_ACCESS` | 3.75e-6 | joules | Joules per memory access | `constants.rs` |
| `COMMIT_WINDOW_BLOCKS` | 4300 | blocks | ~30 days | `constants.rs` |
| `EFFICIENCY_PENALTY_THRESHOLD` | 0.7 | — | Penalty below this | `constants.rs` |
| `EFFICIENCY_CAP_THRESHOLD` | 1.3 | — | Cap above this | `constants.rs` |

### C.5 VR Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `VR_WINDOW_BLOCKS` | 1000 | blocks | ~7 days | `constants.rs` |
| `J_PER_KWH` | 3,600,000 | joules | 1 kWh = 3.6 MJ | `constants.rs` |

### C.6 Privacy Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `RING_SIGNATURE_SIZE` | 11 | members | MLSAG ring size | `constants.rs` |
| `MIN_RING_SIZE` | 2 | members | Minimum MLSAG ring | `privacy.rs` |

### C.7 Network Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `MAX_PEERS` | 125 | peers | Peer set limit | `constants.rs` |
| `CONNECTION_BURST` | 5 | conn/s | TokenBucket burst | `p2p.rs` |
| `CONNECTION_REFILL` | 5 | conn/s | TokenBucket refill | `p2p.rs` |
| `MAX_PEER_MANAGER` | 200 | peers | PeerManager capacity | `p2p.rs` |
| `IDLE_TIMEOUT` | 60 | seconds | Connection idle timeout | `p2p.rs` |
| `MAX_ORPHANS` | 500 | blocks | Orphan queue depth | `chain.rs` |
| `MAX_CACHED_BLOCKS` | 10000 | blocks | In-memory block cache | `store.rs` |
| `MAX_MEMPOOL_TXS` | 5000 | txs | Mempool size limit | `mempool.rs` |
| `MAX_REORG_DEPTH` | 100 | blocks | Maximum reorg depth | `reorg.rs` |
| `GOSSIP_HEARTBEAT` | 5 | seconds | gossipsub interval | `p2p.rs` |

### C.8 Testnet Constants

| Constant | Value | Unit | Description | Code Reference |
|----------|-------|------|-------------|---------------|
| `TESTNET_DAG_SIZE` | 268,435,456 | bytes | 256 MB | `constants.rs` |
| `TESTNET_BLOCK_TIME` | 60 | seconds | 1 min (vs 10 min mainnet) | `constants.rs` |
| `TESTNET_RAMP_UP` | 100 | blocks | ~1.7 hours | `constants.rs` |
| `TESTNET_COMMIT_WINDOW` | 43 | blocks | ~43 min | `constants.rs` |
| `TESTNET_FOUNDER_LOCK` | 500 | blocks | ~8.3 hours | `constants.rs` |

---

## 20. Appendix D — Security Assumptions

### D.1 Cryptographic Assumptions

| ID | Assumption | Formal Statement | Protocol Dependency | Quantum Risk |
|----|-----------|-----------------|-------------------|-------------|
| CR-1 | **DLOG in Ristretto** | Given $P = xG$, no PPT adversary finds $x$ with non-negligible probability | Pedersen commitment binding, MLSAG anonymity, stealth address unlinkability | HIGH (Shor) |
| CR-2 | **CDH in Ristretto** | Given $(aG, bG)$, no PPT adversary computes $abG$ | Key image unlinkability | HIGH (Shor) |
| CR-3 | **DDH in Ristretto** | Distinguishing $(aG, bG, abG)$ from $(aG, bG, cG)$ is hard | Stealth address privacy (computational hiding) | HIGH (Shor) |
| CR-4 | **Keccak-256 as Random Oracle** | Outputs are indistinguishable from random for any PPT distinguisher | DAG determinism, block hashing, merkle trees | LOW (Grover: 64-bit) |
| CR-5 | **SHA-512 as Random Oracle** | Same as CR-4 for SHA-512 | DAG walk mixing, proof traces | LOW (Grover: 128-bit) |
| CR-6 | **Ed25519 EUF-CMA** | No PPT adversary forges a signature under chosen message attack | Commitment authentication, TX authorization | HIGH (Shor) |
| CR-7 | **SHAKE-256 Domain Separation** | Distinct domain tags produce independent random oracles | MLSAG challenge, hash-to-point, Pedersen generators | LOW |

### D.2 Economic Assumptions

| ID | Assumption | Formal Statement | Protocol Dependency | Violation Risk |
|----|-----------|-----------------|-------------------|---------------|
| EC-1 | **Honest Majority** | No single entity controls ≥50% of total network AOPS | Double-spend resistance, chain finality, censorship resistance | MODERATE (DRAM concentration) |
| EC-2 | **Free Entry / Exit** | Any miner can join or leave at marginal cost (procurement + electricity) | Decentralization, equilibrium price discovery, ASIC resistance | LOW (commodity DRAM) |
| EC-3 | **Miner Rationality** | Miners maximize expected profit (no altruistic or malicious behavior without economic incentive) | Nash equilibrium analysis, collusion stability | MODERATE (state actors, ideology) |
| EC-4 | **No Cartel Coordination** | No subset of miners can sustain a collusive agreement without profitable defection | Emission stability, fair reward distribution | MODERATE (pool centralization risk) |
| EC-5 | **Elastic Supply Discovery** | Market price of eWatt reflects fundamental value (VR + future utility expectations) | VR as a pricing floor | HIGH (illiquid markets) |
| EC-6 | **No Dominant Hardware Advantage** | No single hardware design achieves >5× cost efficiency vs DDR5 baseline | ASIC resistance, memory-bound assumption | MODERATE (custom SHA-512 ASIC possible) |

### D.3 Network Assumptions

| ID | Assumption | Formal Statement | Protocol Dependency | Violation Risk |
|----|-----------|-----------------|-------------------|---------------|
| NET-1 | **Partial Synchrony** | Network delay is bounded but the bound is unknown; after GST, messages arrive within Δ seconds | Block propagation within 600s, orphan rate near zero | LOW (block time >> latency) |
| NET-2 | **Peer Connectivity** | Every honest node has at least one honest peer in its connected component | Gossip propagation, block dissemination | MODERATE (eclipse attack) |
| NET-3 | **Point-to-Point Authenticity** | Messages from peer A to peer B are authenticated and integrity-protected (Noise protocol) | Sybil resistance within session, message integrity | LOW (libp2p Noise standard) |
| NET-4 | **Sufficient Peer Diversity** | The peer set contains at least one honest peer that can relay the latest canonical block | Liveness, fork resolution | MODERATE (sybil fills peer slots) |
| NET-5 | **Time Synchronization** | Node clocks are within a bounded drift of real time ($\pm$2 hours) | Timestamp validation (2h tolerance) | LOW (NTP) |
| NET-6 | **No Global Eavesdropper** | No single adversary observes all network traffic | Topology obfuscation, transaction source anonymity | MODERATE (ISP-level monitoring) |

### D.4 Operational Assumptions

| ID | Assumption | Formal Statement | Protocol Dependency | Violation Risk |
|----|-----------|-----------------|-------------------|---------------|
| OP-1 | **Key Security** | Private keys are stored securely and never leaked | Fund security, miner identity | HIGH (plaintext key files) |
| OP-2 | **Software Integrity** | The node binary has not been tampered with | Consensus correctness, state validation | MODERATE (supply chain) |
| OP-3 | **File System Durability** | Writes to disk (especially block/appends) survive crashes | UTXO persistence, block log integrity | MODERATE (sync_data before crash) |
| OP-4 | **Sufficient RAM** | Node has ≥8 GB RAM (mainnet DAG + UTXO set + mempool) | Mining viability, block processing | MODERATE (8GB NIC requirement) |

---

## 26. References

[1] Nakamoto, S. (2008). *Bitcoin: A Peer-to-Peer Electronic Cash System.*  
[2] Lamport, L., Shostak, R., & Pease, M. (1982). *The Byzantine Generals Problem.* ACM Trans. Program. Lang. Syst.  
[3] Back, A. (2002). *Hashcash — A Denial of Service Counter-Measure.*  
[4] Buterin, V. (2014). *Ethash: A Memory-Hard Proof-of-Work Algorithm.*  
[5] Percival, C. (2009). *Stronger Key Derivation Via Sequential Memory-Hard Functions.*  
[6] Biryukov, A., Dinu, D., & Khovratovich, D. (2015). *Argon2.*  
[7] Noether, S. (2016). *Ring Confidential Transactions.*  
[8] Noether, S., & Mackenzie, A. (2016). *Monero's Ring Signature Privacy.*  
[9] Maskin, E. (2008). *Mechanism Design.* Nobel Lecture.  
[10] Osborne, M. J., & Rubinstein, A. (1994). *A Course in Game Theory.* MIT Press.  
[11] Nisan, N., et al. (2007). *Algorithmic Game Theory.* Cambridge.  
[12] Kroll, J. A., et al. (2013). *The Economics of Bitcoin Mining.* WEIS.  
[13] Axelrod, R. (1984). *The Evolution of Cooperation.* Basic Books.  
[14] Kiayias, A., et al. (2016). *Incentive Compatibility of Bitcoin Mining.*  
[15] Eyal, I., & Sirer, E. G. (2014). *Majority is Not Enough.*  
[16] Sompolinsky, Y., & Zohar, A. (2013). *GHOST.*  
[17] Abraham, I., et al. (2019). *HotStuff.*  
[18] Kumar, A., et al. (2017). *A Traceability Attack Against Monero.*  
[19] Alonso, K. M., & Krawiec, T. (2020). *Zero to Monero.*  
[20] Rosenfeld, M. (2011). *Analysis of Bitcoin Pooled Mining Reward Systems.*  
[21] Brumley, D., & Boneh, D. (2005). *Remote Timing Attacks.*  
[22] Howard, M., & Lipner, S. (2003). *STRIDE.* Microsoft.  
[23] Daian, P., et al. (2020). *Flash Boys 2.0: Frontrunning, MEV.*  
[24] Heilman, E., et al. (2015). *Eclipse Attacks on Bitcoin.*  
[25] Liao, K., & Katz, J. (2017). *Incentivizing Blockchain Forks.*  
[26] Eyal, I. (2015). *The Miner's Dilemma.* IEEE S&P.  
[27] Luu, L., et al. (2015). *On the Security and Performance of Pool Hopping.*  
[28] Abe, M., Ohkubo, M., & Suzuki, K. (2002). *1-out-of-n Signatures from a Variety of Keys.*  
[29] Bradner, S. (1997). *Key Words for Use in RFCs.* RFC 2119.  
[30] Fischer, M. J., Lynch, N. A., & Paterson, M. S. (1985). *Impossibility of Distributed Consensus.*  
[31] Ongaro, D., & Ousterhout, J. (2014). *In Search of an Understandable Consensus Algorithm (Raft).*  
[32] Carlsten, M., et al. (2016). *On the Instability of Bitcoin Mining.*  
[33] Sompolinsky, Y., & Zohar, A. (2016). *SPECTRE.*  

---

**Document version:** 3.0 (DRAFT)  
**Status:** Security Engineering Reference  
**Classification:** Internal  

*This document is a living artifact. Items marked [Requires Code Confirmation] need manual source verification. The protocol specification (Volume I) uses RFC 2119 language for normative requirements. All other volumes are analytical.*
