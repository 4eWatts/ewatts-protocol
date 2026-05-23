# Audit: reward.rs — Monetary Policy Engine

> **Auditor:** Gustavo
> **Date:** 23 May 2026
> **Type:** Protocol review — incentive surface + numerical stability
> **Module:** `src/reward.rs`

## 1. Verdict

- ✅ Estrutura geral é boa (bem melhor que 90% de protótipos cripto)
- ✅ Separação int vs float está correta conceitualmente
- ✅ Ramp-up + ceiling + floor existem (bom sinal de consciência de stability design)
- ⚠️ 3 classes de risco sério identificadas

## 2. 🚨 PROBLEMA 1: DUAL SYSTEM (FLOAT vs INT)

Dois mundos:
- **Mundo A (econômico real):** `compute_block_rewards(...)` — f64
- **Mundo B (consenso/migration):** `compute_block_rewards_int(...)` — u64

**Problema estrutural:** Esses dois NÃO são isomórficos garantidos.

**Risco real:**
- rounding divergence entre nodes
- reward drift entre forks
- consensus split via numerical instability

**Onde explode na prática:**
```rust
let c_eff = commitment::effective_commitment(...)
// vs
let r = (c_eff / total_eff) * emission;
```

Isso cria:
- path-dependent rounding
- order-of-sum sensitivity

**Fix necessário (conceitual):** single canonical integer domain BEFORE aggregation. Não dual domain.

## 3. 🚨 PROBLEMA 2: NON-DETERMINISTIC SUM ORDER

```rust
let total_eff = 0.0;
for c in commitments {
    total_eff += c_eff;
}
for (c_eff, mid) in &effective {
    let r = (*c_eff / total_eff) * emission;
}
```

**Problema:** Floating division after accumulation ⇒ associativity drift.

- node-dependent ordering risk
- shuffle/reorg amplification of tiny differences

**Clássico em:** Ethereum pre-merge reward bugs, PoS slashing inconsistencies, cross-client consensus divergence.

## 4. 🚨 PROBLEMA 3: RAMP-UP CAP NÃO É CONSERVATIVO

```rust
let share_exceeds = reward.saturating_mul(CAP_PRECISION)
    > total.saturating_mul(RAMP_UP_CAP_INT);
```

**Problema:** Comparação de individual reward vs global total, mas aplicando:
- `*reward = max_reward;`
- `burned += excess;`

**Risco:** order-dependent mutation effect, partial correction (não redistribui corretamente o excedente global), pode criar implicit supply leakage under multi-miner edge cases.

**Efeito adversarial:** miner ordering changes final distribution, reorg changes burn amount, inconsistent inflation pressure under forks.

## 5. ⚠️ PROBLEMA 4: EMISSION RATE BOUNDARY BEHAVIOR

```rust
if hist_avg == 0 { return BASE_EMISSION_INT; }
```

**Problema:** Cold start regime é discontinuous, non-normalized, not symmetric under network bootstrap.

**Risco:** early chain bias (bootstrap miners advantage), inconsistent initial difficulty economics.

## 6. ⚠️ PROBLEMA 5: VALIDATION FILTER INSIDE REWARD LOOP

```rust
if commitment::validate_commitment(c, previous_commitments).is_err() {
    continue;
}
```

**Problema:** Mixing validation logic with economic weighting logic.

**Risco:** different nodes may interpret "validity set" differently under partial view. Reward computation becomes state-dependent on validation order.

## 7. ✅ O QUE ESTÁ CORRETO

- Emission is bounded (floor + ceiling exist)
- Incentive proportionality exists (c_eff / total_eff design is directionally correct)
- Ramp-up anti-whale mechanism exists (good anti-capture intuition)
- Integer migration is started (passo mais importante do módulo)

## 8. 🔥 CRITICAL SYSTEMIC INSIGHT

> Este módulo é atualmente "economically correct in isolation, but not yet consensus-safe under distributed execution".
