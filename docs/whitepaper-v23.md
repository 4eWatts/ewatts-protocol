# Ewatts Protocol v23 — Memory-Bound Digital Currency

**DRAM-Bound Proof-of-Energy — Whitepaper**
*June 2026*

**Ewatts is not a store of value. It is a ruler.**
Gold and Bitcoin are stores of value — they appreciate in purchasing power over time. Fiat currencies decay — they lose purchasing power over time. Ewatts is neither. It is designed to remain stable in real terms, anchored to the cost of energy production, so that credit markets can function.

When a farmer takes a loan for fertilizer, denominated in Ewatts, the repayment burden in real terms is predictable. When a sovereign issues debt denominated in Ewatts, the buyer knows the principal will retain purchasing power at maturity. This is the problem Ewatts solves: **providing a stable unit of account for energy-denominated credit**, not maximizing token price.

---

## 1. Abstract

Ewatts is a neutral digital currency whose issuance is constrained by **verifiable DRAM bandwidth competition**. Rather than declared energy expenditure (which cannot be verified on-chain), miners commit sustained memory bandwidth — a physical resource that is measurable, auditable, and geographically uniform.

The protocol uses Memory-Bandwidth-Bound Proof-of-Work (MBPoW), where the bottleneck is DRAM bandwidth (improving ~1.2%/year) rather than transistor logic (improving ~100×/decade for SHA-256 ASICs). Energy is not declared — it is inferred from the stable physical relationship between memory access and electricity consumption.

The result is a digitally native asset whose issuance is provably honest. Contracts denominated in kWh are settled via the Valor de Referência (VR), an on-chain derivation that converts bandwidth commitments into an energy reference rate without oracles. **Ewatts serves as infrastructure for energy-denominated credit markets — a stable unit of account in which producers can lend, borrow, and settle without the inflation risk of fiat or the appreciation risk of gold and Bitcoin.**

---

## 2. Key Principles

1. **Immutable Core** — No forks, no upgrades, no governance. The protocol is fixed at genesis.
2. **Energy-Inferred, Not Declared** — The protocol measures memory bandwidth via DAG walk verification. Energy is an emergent property of bandwidth × time, not a declared number.
3. **Geography-Neutral** — Same bandwidth in any country produces the same competitive position. No location matters.
4. **Privacy Baseline** — Ring signatures and stealth addresses by default.
5. **Non-Governance** — No dev funds, no voting, no administrative keys.
6. **Honest Issuance, Not Price Stability** — The energy anchor guarantees that every Ewatt required real energy to produce. It does not guarantee that 1 Ewatt = X kWh in market price. See §7 and §11.

---

## 3. The DRAM Constraint

SHA-256 PoW degrades as an energy proxy because ASICs improve >1,000,000× per watt over 14 years. DRAM bandwidth does not — it improves ~1.2%/year because the energy cost of moving electrons across a physical bus is not accelerated by transistor shrinks.

This physical constraint is the protocol's foundation: bandwidth competition IS energy competition, without requiring miners to declare energy costs. The energy intensity of DRAM access (~0.08 J/GB) is a stable physical constant that does not improve with process node shrinks.

---

## 4. Bandwidth Commitment Model (§3.1/§3.2)

### 4.1 The Core Substitution

v22 replaced kWh declarations with bandwidth commitments. This is the single most important change from earlier versions.

**Problem with kWh declarations:**
- Miners can declare any kWh number. The protocol cannot verify actual electricity consumption.
- Declared kWh is fiction — it relies on trust and external audits, not on-chain proof.
- This creates an unenforceable gap between what miners say and what they spend.

**Solution: bandwidth commitments (GB/s):**
- Miners declare sustained DRAM bandwidth in GB/s.
- Bandwidth is physically verifiable via DAG walk: GB processed ÷ time = sustained bandwidth.
- Proof traces include timestamps for traffic analysis.
- 100 GB/s is identical whether in Paraguay, France, or Japan.
- Energy is then inferred from the fixed physical relationship: 0.08 J/GB.

### 4.2 Commitment Mechanics

Each miner submits a signed bandwidth commitment alongside their block solution:

```
Commitment = {
    miner_public_key,
    declared_gbps,         // Sustained GB/s
    block_number,
    work_gb,               // GB processed through DAG walk
    time_seconds,          // Wall time for the DAG walk
    signature
}
```

### 4.3 Efficiency and Penalties

```
eff = work_gb / (declared_gbps × time_seconds)
```

- **eff < 0.7**: Over-declaration penalty. Effective commitment is reduced proportionally. A miner who claims 100 GB/s but delivers 60 GB/s is penalized.
- **eff > 1.3**: Under-declaration flag. The miner is capped — declaring far below actual bandwidth gains no advantage.
- **0.7 ≤ eff ≤ 1.3**: Honest range. Commitment is accepted at face value.

The honest equilibrium is to declare sustained bandwidth ± margin. Over-declare → penalty. Under-declare → capped. The system self-corrects without governance.

### 4.4 Why This Matters

Energy is the underlying cost of mining — it costs joules to access DRAM. But the protocol cannot measure joules directly. By measuring what it can measure (bandwidth) and inferring what it cannot (energy), the protocol achieves honest issuance without trusted third parties.

---

## 5. Founder Mining (§1b)

### 5.1 Problem: Bootstrap Without Pre-Mine

A pre-mine creates an insider cohort with tokens at zero cost, violating the principle of honest issuance. But a Proof-of-Work chain with zero initial value has no security — no one mines an empty chain.

### 5.2 Solution: Founder Mining

Instead of a pre-mine, the founder mines the first blocks with a **reputational collateral**:

- The founder commits to a publicly known key and mines alone during the bootstrap phase.
- Block rewards during this phase go to the founder, creating the initial supply.
- The founder's collateral is reputational: their identity and stake in the project's success serve as the guarantee against dumping.
- After the ramp-up period (10,000 blocks, ~70 days), founder mining ceases and the network opens to permissionless mining.

### 5.3 Properties

- No tokens are created at zero cost. Every Ewatt the founder holds required real bandwidth (and therefore real energy) to produce.
- The founder's mining activity is transparent and auditable — every block is on-chain.
- The ramp-up period (~70 days) aligns incentives: dumping early would destroy the value of later blocks.

### 5.4 Bootstrap Risk: VR Manipulation in Low-Adoption Networks

During bootstrap, a single miner with ~50% of total bandwidth can influence the VR through strategic over-declaration. Example:

- Founder mines at 50 GB/s (honest), attacker declares 100 GB/s but delivers 50 GB/s (η = 0.5).
- After penalty, effective commitment drops to 50 GB/s — same as founder.
- But the VR numerator was inflated by 50 GB/s for that block.

**Mitigation**: VR window (1,000 blocks) dilutes single-block manipulation. Ramp-up caps (±1% bandwidth change per block) limit acceleration. Formal game-theoretic analysis is required before mainnet. For the bootstrap period, contract settlement should not reference VR until sufficient miner diversity exists.

---

## 6. Protocol Architecture

### 6.1 Emission

```
R(block) = BASE_EMISSION × (Total_Commitment / Historical_Avg_Commitment)
```

Where `Historical_Avg_Commitment` = mean of total bandwidth commitments over the last 1,000 blocks.

**Bounds:**
- `R_min = BASE_EMISSION × 0.1` (floor: 10% of base)
- `R_max = BASE_EMISSION × 10` (ceiling: 10× base)

### 6.2 Reward Distribution

```
Reward_i = (eff_commit_i / Σ eff_commit_j) × R
```

Where:
- `eff_commit_i` = miner's effective bandwidth commitment (GB/s, after penalty/cap)
- `R` = current emission rate

Reward is proportional to effective bandwidth share only. The efficiency penalty system (§4.3) already ensures that over-declaration reduces effective bandwidth, so no additional work-weighting is needed. Bandwidth × time is work, and the protocol measures both; but the penalty mechanism makes double-weighting redundant.

**Properties:**
- Σ reward_i = R (all emission is distributed, no residual)
- Over-declaration reduces ce via penalty, reducing reward proportionally
- Under-declaration provides no benefit (cap at 1.3×)
- Honest declaration at true bandwidth is the unique Nash equilibrium

**Earlier spec versions** used inverse weighting `(1/ce)/Σ(1/ce) × (W_i/W_t)`, which was removed after game-theoretic analysis proved it rewarded under-declaration (a miner claiming 10 GB/s while delivering 100 GB/s would receive 7.7× more reward than an honest miner). Double-weighted formulas `(ce/Σce) × (W_i/W_t)` were also rejected because they leave a residual — the product of two distributions does not sum to 1 unless perfectly correlated, which efficiency penalties break.

### 6.3 Properties

- **Linear emission**: Supply grows proportionally to bandwidth demand. If total committed bandwidth doubles, emission doubles. No predetermined halving schedule, no artificial scarcity.
- **Self-correcting**: Falling bandwidth commitments automatically reduce emission.
- **No early-adopter bias**: The linear formula ensures no structural advantage for early miners.

---

## 7. ASIC Resistance (§4.5)

### 7.1 Physical Basis

ASIC advantage in MBPoW is estimated at 3-5× (unproven above 5×). The DRAM bandwidth bottleneck limits ASIC gains because:
- Custom memory controllers may achieve slightly better latency/power than commodity DDR5.
- But the fundamental constraint is DRAM bus speed, which is dominated by the same 3 manufacturers (Samsung, SK Hynix, Micron) that serve the commodity market.

### 7.2 DAG Growth

The DAG grows by 0.5 GB/year. An ASIC with fixed on-package DRAM becomes obsolete in ~3-4 years as the working set exceeds its capacity.

### 7.3 Self-Executing Contingency

If community benchmarks confirm >5× ASIC advantage for 2 consecutive years, DAG growth accelerates to 1.0 GB/year. **This is automatic — hard-coded in the genesis parameters, not a governance vote.**

The detection mechanism:
- On-chain monitoring of average efficiency (`avg_η_epoch`)
- Trigger: `avg_η_epoch > 1.3 AND measured bandwidth > 100 GB/s` for 2 consecutive years
- Once triggered, the faster growth rate is permanent

This is the protocol's only built-in response to external conditions. It is deterministic and non-governable.

---

## 8. What Ewatts Is and Is Not

**Is:** A monetary system constrained by irreducible memory movement. Issuance is honest because bandwidth is physically limited. The energy link is emergent (bandwidth × time × 0.08 J/GB), not pegged.

**Is not:** An electricity tracker, a stablecoin, an energy receipt, or a price-predictable instrument. The energy anchor guarantees *issuance integrity*, not *price stability*. Market value is dominated by adoption and monetary demand — exactly as with gold and Bitcoin.

---

## 9. Privacy and Transactions

Ring signatures and stealth addresses by default. Transaction amounts are obfuscated via Pedersen commitments. The protocol provides a privacy baseline comparable to Monero, without optional transparency.

For users who require auditability (regulated entities), voluntary disclosure proofs are supported — a user can prove a specific transaction to an auditor without revealing all activity.

---

## 10. On-Chain Contract Settlement

### 10.1 Problem: Volatility

A neutral digital currency is useful for cross-border trade only if both parties can agree on the value at signing and settlement. Token price volatility (driven by adoption and speculation) makes direct Ewatt-denominated contracts unreliable.

### 10.2 Solution: Dual-Denomination via VR

Contracts are **denominated in kWh** (the real unit of energy) and **settled in Ewatts** at the VR of the settlement block.

At signing:
- Party A and B agree: "100,000 kWh of fertilizer, delivered in 90 days"
- The contract is recorded on-chain with the kWh amount, not the Ewatt amount

At settlement:
- The protocol reads VR(block) for the settlement block
- `Ewatt_delivery = kWh_obligation / VR(block)`
- Party A delivers that many Ewatts to Party B

### 10.3 Properties

- **No oracles**: The VR is derived entirely on-chain from bandwidth commitments. No price feed, no trusted third party.
- **Predictable for energy-denominated parties**: If both counterparties have costs and revenues in energy terms, the VR divergence from kWh spot is 0.7-1.2% annualized.
- **Volatility isolation**: The Ewatt/USD price can double without affecting the contract — the obligation is always in kWh.

### 10.4 Limitations (Transparent Disclosure)

The VR is a rolling 1000-block average (~7 days) of bandwidth-driven issuance. Market price reacts instantly to adoption shocks and monetary demand. In a 90-day contract window, the token price in USD and the VR can diverge materially.

Simulation results across 7 market regimes (including sanction proliferation and SWIFT fragmentation scenarios):

| Metric | Value |
|--------|-------|
| VR annualized volatility | 0.7-1.2% |
| Contract slippage (90d, USD-denominated) | 25-50% median |
| Contract slippage (90d, kWh-denominated) | 7-15% median |
| Contract slippage (60d, kWh-denominated) | 5-10% median |
| Contract slippage (30d, kWh-denominated) | 2-5% median |
| Contract slippage (14d, kWh-denominated) | <1% median |
| Tranched settlement (3×30d tranches for 90d contract) | 5-10% median |

**The important distinction**: The 25-50% figure measures USD-translation noise, not real purchasing power erosion. For counterparties whose costs and revenues are energy-denominated (fertilizer, soy, oil, electricity), the relevant metric is kWh-denominated slippage — 7-15% at 90 days.

**Mitigation strategy**: Keep contract tenors ≤30 days for slippage within 2-5%. For longer tenors, use tranched settlement (multiple tranches at 30-day intervals) rather than lump-sum at expiry. For tenors ≥90 days, a forward VR hedge is recommended — not optional (see §14).

---

## 11. Valor de Referência (VR) — §11b

### 11.1 Definition

The VR (Valor de Referência) is an on-chain reference rate that expresses how many joules of proven energy expenditure were required per Ewatt issued in the recent epoch.

```
VR(block) = (Σ bandwidth_commitments × Δt × 0.08 J/GB) / Σ Ewatts_mined
```

Where:
- `Σ bandwidth_commitments` = sum of verified commitments in the window (GB/s)
- `Δt` = time interval (seconds)
- `0.08 J/GB` = fixed physical constant (energy cost of DRAM access)
- `Σ Ewatts_mined` = total issuance in the same window

### 11.2 Window

The VR is calculated over a rolling window of the last 1,000 blocks (~7 days). This provides statistical confidence while remaining responsive to network changes.

### 11.3 Properties

- **Oracle-free**: Everything is derived from on-chain data. No external price feed, no trusted third party.
- **Physically anchored**: The 0.08 J/GB constant is a physical property of DRAM, not a protocol parameter.
- **Stable**: Simulation across 7 market regimes shows annualized volatility of 0.7-1.2%. Compare: gold ~13%, EUR/USD ~7%, USDT ~0.5%.
- **Public**: Anyone can compute VR(block) from the blockchain. No API key, no subscription.

### 11.4 Warning: VR Is Not a Price

The VR expresses energy cost, not market value. An Ewatt may trade at USD $10 (market price) while VR indicates $0.05/kWh equivalent. The gap between spot price and VR is the "adoption premium" — the market's bet on future utility over current energy cost.

**Two prices coexist:**
- **Spot (Ewatt/USD)**: What the market believes Ewatt is worth. Reacts in seconds to news, adoption, speculation, fear.
- **VR (kWh/Ewatt)**: What the network proves it cost to produce. Reacts in days, via bandwidth verification.

Unlike commodity markets (crude oil, natural gas), there is no forced convergence mechanism between spot and VR. You cannot buy Ewatt cheap on spot and burn it to mine — mining is proof-of-bandwidth, not proof-of-burn. The two prices can diverge for extended periods.

**What the gap means:**
- `Spot >> VR`: The token carries high speculative premium (most cryptocurrencies today)
- `Spot ≈ VR`: The token has achieved commodity-energy maturity
- `Spot << VR`: The protocol is undervalued vs production cost — unsustainable long-term (no one mines at a loss)

---

## 12. Bridging and Interoperability

### 12.1 Native to External Chains

Ewatts is a standalone L1. To access liquidity from other ecosystems, a bridge mechanism is required:

**Wrapped Ewatt (ERC-20):**
- A bridge operator holds native Ewatt in a verifiable address.
- An equivalent amount of wrapped Ewatt (wEWATT) is minted on Ethereum.
- Burning wEWATT releases native Ewatt back to the holder.
- The bridge operator charges a small fee (0.1-0.3%) for mint/burn operations.

**Trust model:** Users trust the bridge operator to not abscond with native collateral. Over time, a trustless bridge using light client proofs over Ethereum can replace the operator model — but this requires significant development effort.

### 12.2 Fiat On/Off Ramps

Ewatts gains utility only when users can enter and exit the system:

- **Exchange listing**: Centralized exchanges (Binance, Coinbase, regional BR exchanges) provide the primary fiat gateway. Liquidity and compliance are the barriers, not technology.
- **Payment processors**: Integration with MoonPay, Onramp, or local BR equivalents allows credit card / PIX purchase.
- **Direct P2P**: Atomic swaps or escrow-based OTC desks for high-value B2B settlement.

### 12.3 Architecture Note: Not Forks

Integration is not achieved through forks. A fork creates a separate chain variant — fragmentation, not interoperability. Integration is achieved through:
- **Wrapped assets** (bridge to Ethereum ecosystem)
- **Exchange listings** (fiat bridge)
- **Standard APIs** (RPC, REST endpoints for wallet and explorer builders)

---

## 13. Business Model

### 13.1 Core vs Edge

The protocol itself is public good — permissionless, free, open source. No one pays to use the L1. Revenue is generated at the edges:

| Layer | Revenue Model | Operator |
|-------|---------------|----------|
| L1 Protocol | Free, permissionless | None (public good) |
| Bridge (native → ERC-20) | 0.1-0.3% mint/burn fee | Foundation or third party |
| Fiat ramp | Spread on entry/exit | Exchange partner |
| Contract settlement UI | SaaS fee or per-contract fee | Foundation or third party |
| VR dashboard / explorer | Free (public good) | Foundation |

### 13.2 Why This Works

The bridge generates network effects for the core. Each time a user wraps Ewatt to Ethereum (or unwraps back), the L1 gains liquidity and users. The business feeds the protocol.

This is the same model as Ethereum + Infura: the L1 is free, infrastructure built on top is monetized. The difference is that in Ewatts, the foundation (or the founder) operates the bridge and settlement layer, capturing value within the ecosystem rather than ceding it to a third party.

### 13.3 No Token Sale

Ewatts has no ICO, no pre-sale, no venture allocation. The only Ewatts in existence are those mined by the founder during bootstrap (§5) or by permissionless miners after ramp-up. This is a design choice: supply emerges from real work, not from a financial instrument.

---

## 14. Hedging and Risk Management

### 14.1 The Problem

Counterparties whose final obligations are in fiat (USD, BRL, EUR) face exchange rate risk even when contract terms are kWh-denominated. The VR protects against energy price divergence, but not against fiat devaluation / Ewatt appreciation.

### 14.2 Forward VR Contracts

The protocol does not natively support hedging instruments. However, the fixed VR formula enables counterparties to enter forward agreements:

- Party A and B agree on a forward VR for settlement block N: `VR_forward(N)`
- If actual VR(N) > VR_forward, Party B pays Party A the difference in Ewatt
- If actual VR(N) < VR_forward, Party A pays Party B

This is a derivative, not a protocol feature — but the VR's deterministic and predictable behavior (0.7-1.2% vol) makes forward pricing feasible.

### 14.3 Overcollateralized kWh-Stable Asset

A secondary token pegged to a basket of kWh can be built on top of Ewatts (similar to DAI on Ethereum):
- Overcollateralize with Ewatt at ~150-200%
- Issue a stable token pegged to kWh
- The VR provides the price feed

This is not in the current spec. It is a future extension that becomes viable once Ewatt has sufficient liquidity.

### 14.4 Best Practice for B2B Users

For parties with energy-denominated costs and revenues (agriculture, fertilizer, oil, electricity):
- Use kWh-denominated contracts (not Ewatt or USD)
- Keep contract tenors ≤30 days (≤3% slippage)
- Use tranched settlement for longer commitments (settle 50% at 30d, 50% at 60d)
- If fiat exposure is unavoidable, hedge via forward VR or options on a compatible exchange

For parties with fiat-denominated obligations (most SMEs, service providers):
- The VR reduces but does not eliminate currency risk
- The recommended approach is to treat Ewatt as a settlement rail (receive → convert immediately), not as a store of value

---

## 15. UX and Onboarding

### 15.1 The Onboarding Gap

The protocol's technical design is sound. The user's experience is not. A farmer in Mato Grosso or an SME in São Paulo will never read §11b or compute VR(block). For Ewatts to reach real users, a UI layer must abstract the complexity.

### 15.2 Required Infrastructure

| Component | What It Does | Who Builds It |
|-----------|--------------|---------------|
| Wallet | Send/receive Ewatt, manage keys | Foundation (reference) |
| Explorer | View blocks, transactions, VR history | Foundation (reference) |
| Contract UI | Create kWh-denominated contracts, see VR conversion automatically | Foundation (MVP) |
| Exchange frontend | Buy/sell Ewatt with fiat | Exchange partner |
| Bridge UI | Wrap/unwrap Ewatt to ERC-20 | Foundation or third party |
| Settlement dashboard | For B2B users: manage counterparties, contracts, settlement history | Foundation |

### 15.3 UX Principles

- **Default to hiding VR**: The typical user should never see VR. The contract UI shows "10,000 kWh" and the Ewatt equivalent automatically.
- **Default to simple**: One balance, one send button, one receive address.
- **Progressive disclosure**: Advanced users (miners, B2B traders) can access VR charts, bridge controls, and settlement history.
- **Localized**: Portuguese, Spanish, English, Mandarin at minimum.

### 15.4 KYC/AML Integration

For regulated use cases (B2B trade, exchange listing), the protocol does not enforce KYC at the consensus layer. KYC happens at the bridge and exchange layers:
- Exchange: standard KYC for fiat on/off ramp
- Bridge: optional for small amounts, KYC for high-value wrap/unwrap
- L1: permissionless, no KYC

---

## 16. Regulatory Framework

### 16.1 Jurisdictional Risk

Ewatts operates globally without a legal entity. This is a feature (neutrality) and a risk (no one to sue, no one to regulate).

### 16.2 Likely Regulatory Positions

| Jurisdiction | Likely Classification | Risk |
|--------------|----------------------|------|
| United States (SEC) | Commodity (like Bitcoin), not security — no ICO, no pre-mine, no dev fund | Low |
| Brazil (CVM/BCB) | Likely commodity or payment instrument | Low to moderate |
| EU (MiCA) | Likely falls under crypto-asset regulation, possibly ART if used for payments | Moderate |
| China | Likely banned, as with all permissionless crypto | High (but irrelevant for target users) |
| Russia | Likely tolerated given sanctioned status and need for neutral settlement | Low to favorable |

### 16.3 AML/CFT Exposure

The protocol's privacy features (ring signatures, stealth addresses) create AML/CFT exposure for intermediaries (exchanges, bridge operators), not for the protocol itself. Standard AML controls apply at the bridge and exchange layers.

### 16.4 Tax Treatment

- **Mining**: Taxed as income at receipt (fair market value in local currency).
- **Trading**: Taxed as capital gain/loss on disposal.
- **Contract settlement**: Likely treated as barter transaction — kWh-denominated contract swapped for Ewatt.

Jurisdictions vary. Users should consult local tax professionals. The protocol provides no tax advice.

### 16.5 Recommended Approach

- Register the bridge entity in a compliant jurisdiction (Switzerland, Singapore, or a well-regulated BR hub).
- Maintain KYC/AML at the bridge layer, not the L1.
- Publish regular transparency reports for bridge collateral.
- Engage proactively with regulators in target markets (Brazil first, given the founder's location and the agricultural use case).

---

## 17. Network Effects and Adoption Strategy

### 17.1 The Adoption Filter

Ewatts's most elegant feature — the kWh framing — is also its adoption bottleneck. Users must understand or trust the energy-denominated mental model. This filters out casual speculators and retail investors, but aligns precisely with:

- Agricultural commodity traders (soy, corn, fertilizer)
- Energy traders (oil, gas, electricity)
- Cross-border B2B manufacturers in FX-restricted regimes
- Sovereign entities seeking neutral reserve diversification

### 17.2 Recommended Launch Sequence

| Phase | Duration | Activities |
|-------|----------|------------|
| 1. Bootstrap | Genesis + ~70 days | Founder mining. Reference wallet, explorer, VR dashboard. Closed test with 3-5 B2B counterparties. |
| 2. Permissionless | Post ramp-up | Open mining. Bridge to Ethereum testnet. Exchange listing discussions. |
| 3. Liquidity | Month 3-6 | Bridge to Ethereum mainnet. First CEX listing (regional BR exchange). 10-20 B2B counterparties. |
| 4. Scale | Month 6-12 | Major CEX listing. Payment processor integration. Contract settlement UI v1. |
| 5. Maturity | Year 2+ | Trustless bridge. DeFi integration (lending, AMMs). Sovereign engagement. |

### 17.3 Minimal Viable Network

Ewatts can function with:
- 1 miner (during bootstrap)
- A reference wallet
- A bridge to Ethereum
- A settlement UI
- 2+ B2B counterparties in the same supply chain

Everything beyond this is acceleration, not validation.

---

## 18. Comparison with Alternatives

### 18.1 Bitcoin

| Dimension | Bitcoin | Ewatts |
|-----------|---------|--------|
| Energy anchor | Declared (unverifiable) | Inferred from bandwidth |
| ASIC resistance | None (100M× efficiency gain) | ~3-5× bounded |
| Supply curve | Halving (artificial) | Linear (market-driven) |
| Privacy | Pseudonymous only | Ring signatures + stealth |
| Contract settlement | None | VR-based kWh settlement |
| Governance | Social consensus (forks) | Immutable core |

### 18.2 Gold

| Dimension | Gold | Ewatts |
|-----------|------|--------|
| Energy cost to produce | ~$1,700/oz (~40% of spot) | VR-derived (transparent) |
| Verifiability | Requires assaying | On-chain proof |
| Settlement | Physical transport, T+2 | Instant, final |
| Neutrality | Territorial, jurisdictional | Protocol-enforced |
| Supply growth | 1-2%/year (geological) | Linear, adjustable to demand |

### 18.3 IMF SDR

| Dimension | SDR | Ewatts |
|-----------|-----|--------|
| Composition | 5 fiat currencies | Energy bandwidth |
| Issuance | IMF board vote | Formulaic, automatic |
| Use | Central banks only | Anyone |
| Physical anchor | None | 0.08 J/GB DRAM |
| Governance | Political | None |

---

## 19. Risks and Limitations

1. **Contract slippage**: 25-50% USD translation risk on 90d contracts. Mitigated by short tenors and kWh-denominated settlement. Real kWh-denominated slippage is 7-15% at 90d (validated via Monte Carlo across 7 market regimes). Tranched settlement reduces this to 5-10%. See §10.4 and §14.

2. **kWh PPP retention asymmetry**: Over 15 years, kWh-denominated purchasing power retention ranges from -4% (favorable regimes) to -22% (Energy Crisis Persistent). The VR anchors to electricity cost, not to the broader economy. For sovereign reserve use cases, this is a structural limitation.

3. **Bootstrap VR manipulation**: In low-adoption networks, a single miner with ~50% of bandwidth can influence the VR through strategic over-declaration. Partially mitigated by VR window (1,000 blocks) and ramp-up caps, but formal game-theoretic analysis is pending.

4. **Adoption filter**: The kWh mental model is narrow. Scaling requires onboarding counterparties who think in energy terms. See §17.1.

5. **Bridge trust**: Current design requires a trusted operator. Trustless bridge is high-cost. The bridge operator is also a sanctions target — directly conflicting with the protocol's positioning. See §12.1.

6. **Sanctions exposure of bridge operator**: Any regulated entity running the bridge may be compelled to block transactions involving sanctioned jurisdictions.

7. **Regulatory uncertainty**: Privacy features invite scrutiny. KYC/AML at bridge layer may not satisfy all regulators. See §16.

8. **ASIC development**: Unproven ASIC advantage >5× could outpace DAG growth compensation. Two-year detection window is conservative but slow. See §7.3.

9. **No governance**: Immutability is a feature, but if a critical bug is discovered, there is no fix mechanism. See §2.

10. **VR ≠ price stability**: The VR is a production cost reference, not a price peg. Confusing the two leads to risk mismanagement. See §11.4.

---


## 21. Correlated Collapse Simulation

### 21.1 Purpose

To test Ewatts's resilience under simultaneous sovereign debt crisis, synthetic collateral deleveraging, and accelerated monetary fragmentation. 500 Monte Carlo paths, 15-year horizon. Primary metric: real purchasing power vs commodity basket (t₀=100). Benchmarks: USD, BRL, EUR, Gold.

### 21.2 Results

| Regime | USD | BRL | EUR | Gold | Ewatts | Best |
|--------|-----|-----|-----|------|--------|------|
| System Holds (3%/yr hazard, ~40% peak) | 10x | 5x | 15x | 80x | **397x** | Ewatts |
| Slow Burn (8%/yr hazard, ~80% peak) | 3x | 1x | 5x | 200x | **624x** | Ewatts |
| Full Cascade (2%/yr hazard, ~100% peak) | 0.3x | 0.1x | 0.5x | **3300x** | 208x | Gold |

### 21.3 Reading the Results

**Ewatts does not appreciate. Gold appreciates relative to Ewatts. Bitcoin appreciates relative to Ewatts. Fiat decays relative to Ewatts. Ewatts is the ruler.**

The 397x, 624x, and 208x figures do not mean "Ewatts went up." They show that Ewatts partially captured adoption-driven purchasing power while remaining more stable than any alternative. The ideal outcome for the protocol is Ewatts trading near VR (~stable purchasing power), with the minimum possible speculative premium. Any deviation (397x, 624x) is adoption noise that the VR exists to isolate from credit contracts.

**Key findings:**
1. **Fiat collapses in all regimes.** Full Cascade desintegrates USD to 0.3x, BRL to 0.1x. This is not a financial crisis — it is a paradigm shift in which the entire fiat architecture fails.
2. **Gold dominates extreme collapse** (3300x) because physical asset preservation (~5000 years of track record) outweighs digital infrastructure risk. This is correct — gold is a store of value; Ewatts is not.
3. **Ewatts is most useful where credit markets are most needed** — in energy supply chains where counterparties need a stable unit of account, not the best-returning asset. A 5-year loan for fertilizer equipment denominated in kWh via VR is viable precisely because Ewatts does not appreciate 40%/year.

### 21.4 Portfolio Implications

A balanced reserve for energy supply chains:
- **Gold**: store of value allocation (crisis hedge)
- **Bitcoin or equivalent**: digital store of value (adoption hedge)
- **Ewatts**: unit of account for operational credit (production hedge)

The three assets are complementary, not competing. Ewatts exists to make credit markets function; gold exists to preserve wealth through extreme events. Confusing the two roles leads to bad risk management.

---

## 22. Conclusion

Ewatts v23 is a protocol designed for honest issuance and neutral settlement. Its innovations — bandwidth commitments replacing kWh declarations, VR as on-chain energy reference, linear market-driven emission, and DAG-based ASIC resistance — solve problems that Bitcoin and SHA-256 PoW left unaddressed.

The protocol is not a solution for everyone. It is a solution for parties who:
- Denominate their economics in energy terms (agriculture, fertilizer, oil, electricity)
- Need neutral cross-border settlement without SWIFT dependency
- Accept that a non-governed system has no recourse and no bailout

For these parties, Ewatts offers something no existing protocol provides: a verifiable link between unit of account and physical energy expenditure, without oracles or trusted third parties.

The whitepaper describes what the protocol does. The experiment — building the bridge, onboarding the first counterparties, settling the first cross-border energy-denominated contract — will tell us whether it matters.

---

*Ewatts Protocol v23 — June 2026*
