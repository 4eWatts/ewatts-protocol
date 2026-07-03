# HeyGen Video Revisions — Free Entry Equilibrium Model v2

## Summary

The economic model shift from "programmed cost anchor" to "free entry equilibrium" requires updates to 3 of 4 eWatts promotional videos. One video (Evolution of Money) is unaffected.

| Video | Action | Priority |
|-------|--------|----------|
| The Three-Way Balance | **New script** (11 cards, ~60s) | High |
| The Physics of Value | 3 line corrections | High |
| Hash vs RAM | 2 minor adjustments | Low |
| The Evolution of Money | None | — |

---

## 1. The Evolution of Money — No Changes

**File:** `The_Evolution_of_Money_eWatts-caption_4---5e976746-a24d-4be0-9e85-4d66d2c4db81.srt`

**Assessment:** Safe. All lines remain consistent with the v2 economic model.

Key line that survives scrutiny:
- Line 6: *"a neutral settlement layer anchored to the physical cost of electricity."* Under v2 the anchor is indeed to the marginal cost of electricity (VR = φ·P/p_elec). DRAM latency is what prevents the cost from melting away, but the anchor itself is electricity cost via free entry — exactly what this line says.

No edits needed.

---

## 2. The Physics of Value — 3 Corrections

**File:** `eWatts_The_Physics_of_Value_V3_Refined_-caption---8546a9f7-e237-4788-8412-cc840da4a573.srt`

### Correction 1 — Line 9 (00:30,551 → 00:34,230)

**Original:**
> During bootstrap it's intentionally lower to build the network.

**Problem:** The emission is always 100 eWatt/block. It is never "intentionally lower" during bootstrap. What is low during bootstrap is the *Value Reference* (kWh per eWatt) — because few nodes exist, each token embodies very little energy. This phrasing implies the protocol programs lower issuance, which is inaccurate.

**Revised:**
> During bootstrap, the network is small, so each token costs very little energy. As the network grows, the cost converges to its equilibrium.

### Correction 2 — Lines 10-11 (00:34,230 → 00:39,693)

**Original:**
> As the network matures, the cost per unit converges toward the underlying hardware physics.

**Problem:** Under v2, the cost converges to φ·P/p_elec, which depends on market price and electricity cost — not just hardware physics. DRAM latency is what prevents the cost from melting away (the barrier), not what it converges to (the equilibrium).

**Revised:**
> As the network matures, the cost converges to a simple ratio: market price divided by electricity cost. Hardware efficiency disappears from the equation.

### Correction 3 — Line 12 (00:39,693 → 00:45,104)

**Original:**
> The energy estimate is a conservative protocol parameter — a ruler, not a promise.

**Problem:** The VR is not a "conservative parameter" — it is computed from declared bandwidth × J_PER_GB (a consensus constant). The phrase "ruler, not a promise" is good conceptually but needs precision about what is being measured and how.

**Revised:**
> The Value Reference is a computational meter, not a promise. It measures energy spent, verified from the mining proofs. No oracle, no price feed.

### Lines that remain unchanged:

- Line 8 (00:28,313 → 00:30,351): *"Emission cost is not fixed."* — Correct under v2. Keep.
- Lines 12-17 (00:45,104 → 00:57,458): *"What matters is that every miner faces the same physical constraint..."* through *"The same equation for everyone."* — DRAM access physics is unaffected. Keep.
- Lines 17-19: Currency anchored to verifiable energy cost, privacy features — all correct. Keep.

---

## 3. Hash vs RAM Processing — 2 Adjustments

**File:** `The_Physics_of_Digital_Value_Hash_vs_RAM_Processing_caption_---9d58524a-d868-4915-b630-f0ea378473ff.srt`

### Adjustment 1 — Line 44 (00:17,525 → 00:20,069)

**Original:**
> Every unit issued by RAM processing costs a predictable amount of energy to produce.

**Problem:** "Predictable amount" reads as a fixed number (e.g., ~X kWh per token). Under v2, the energy per token converges to φ·P/p_elec — a predictable *relationship*, not a fixed value. The cost floor doesn't melt away (unlike hash-based PoW), but it does move with market conditions.

**Revised:**
> Every unit issued by RAM processing costs energy that tracks its market value — stable where hash melts away.

### Adjustment 2 — Line 46 (00:20,069 → 00:23,830)

**Original:**
> This is the physical cost anchor used for eWatts token emission.

**Problem:** "Physical cost anchor" implies DRAM latency itself is the value anchor. DRAM latency is the access barrier that makes the proof memory-bound (preventing ASIC dominance). The value anchor under v2 is the marginal cost of electricity via free entry.

**Revised:**
> This is the proof mechanism behind eWatts token emission.

### Lines that remain unchanged:

- Lines 47-48: *"When the cost to mine a coin doesn't fall every year... a currency that measures energy."* — Still true. Efficiency drift is absorbed by N, not the anchor. Keep.
- All lines describing DRAM latency physics, DAG, library metaphor, DDR5 vs timing — unchanged. Keep.

---

## 4. The Three-Way Balance — New Script (Full Rewrite)

**File:** `eWatts_The_Three-Way_Balance-caption_2---533d2acf-ff3d-45dc-ab2a-fe93654b1afe.srt`

**Problem:** The "three forces" framing (latency, efficiency, bandwidth as competing forces) is no longer valid. Efficiency is not a force that needs counterbalancing — it is absorbed by network size (N) through free entry. The entire narrative must be replaced.

**New script (11 cards, ~60 seconds):**

| Card | Time | Text |
|------|------|------|
| 1 | 00:00-00:02 | eWatts: The Free Entry Equilibrium |
| 2 | 00:02-00:08 | Every node running eWatts is a continuous bid in an implicit auction. The block reward is fixed at 100 eWatts. |
| 3 | 00:08-00:14 | The first anchor is latency. Memory access time has been essentially flat for twenty years. The physics does not change with each new chip generation. |
| 4 | 00:14-00:20 | The second anchor is free entry. When hardware gets more efficient, mining becomes more profitable. |
| 5 | 00:20-00:26 | New nodes join. The network absorbs the efficiency gain as more participants, not as cheaper tokens. |
| 6 | 00:26-00:32 | In equilibrium, the cost per token is simply the market price divided by the electricity price. Hardware efficiency cancels out. |
| 7 | 00:32-00:38 | This is the same logic as gold. Better equipment extracts more gold per year, but the cost per ounce is set by the marginal mine. |
| 8 | 00:38-00:44 | On eWatts, the marginal node is any computer with RAM. Entry and exit take minutes, not months. |
| 9 | 00:44-00:50 | No ASIC required. No special hardware. The barrier is zero, so the competition is perfect. |
| 10 | 00:50-00:55 | The result is a stable energy anchor. Efficiency improvements grow the network, not weaken the currency. |
| 11 | 00:55-01:00 | Learn more at eWatts.org. |

**Notes for HeyGen rendering:**
- If the original duration was ~70s, add subtle pauses between cards to match the original timing.
- Visual style suggestion: replace the "three forces balancing" visual with a single line pointing to an expanding network (nodes multiplying as efficiency improves) with the cost anchor line staying flat.

---

## Appendix: Derivation Reference

In case the HeyGen producer asks for context:

```
VR = φ · P / p_elec          (kWh per eWatt at equilibrium)
N* = 8,000 · P / p_elec      (equilibrium nodes, W=75W)
E = 100 · P / p_elec         (kWh per block at equilibrium)
```

Where:
- P = eWatt market price ($)
- p_elec = marginal miner's electricity cost ($/kWh)
- φ = electricity share of marginal cost (~0.85 for commodity hardware)
- W = node wall power (cancels in VR and E)

Hardware efficiency improvements are absorbed by N, not by the anchor. A 5% efficiency gain allows 5% more nodes at the same total energy. The anchor stays flat.
