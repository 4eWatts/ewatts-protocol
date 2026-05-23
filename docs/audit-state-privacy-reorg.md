# Audit: UTXO + Privacy + Reorg Engine

> **Auditor:** Gustavo
> **Date:** 23 May 2026
> **Type:** Structural audit — ledger, privacy, fork safety
> **Modules:** `state.rs`, `privacy.rs`, `reorg.rs`

## 1. Invariantes Críticos (Estado Global)

### Invariantes esperados

O sistema assume implicitamente:
- Σ inputs >= Σ outputs (exceto coinbase)
- total_supply consistente com diffs aplicados
- spent_key_images impede double spend global
- Reorg via BlockDiff é reversível 100%
- MLSAG garante exclusividade de input sem revelar qual UTXO foi gasto

---

## 2. PROBLEMA 1 — Dupla Autoridade de Estado (Split Brain)

Dois sistemas concorrentes de verdade:
- **spent_key_images** + `self.utxos.remove` (privacy domain)
- **UTXO map** (accounting domain)

**Problema:** O sistema depende simultaneamente de key images e UTXO map, mas eles não estão formalmente ligados por uma única invariância verificável.

**Consequência:** Em cenário de fork + reorg parcial:
- key image pode estar removido
- UTXO pode ter sido restaurado via diff
- MLSAG não garante correspondência reversa explícita

**Risco:** Phantom re-spend class bug em reorg concorrente.

---

## 3. PROBLEMA 2 — Reorg Incompleto com MLSAG

`unwind_with_diff()` restaura:
- UTXOs ✅
- key_images ✅
- supply ✅

**MAS NÃO restaura:**
- estrutura de ring membership validity ❌
- stealth linkage consistency ❌
- qualquer estado derivado de `verify_mlsag` ❌

**Risco teórico:** Se dois forks têm mesmos UTXOs mas diferentes ordenações de ring construction → "valid state individually, invalid globally consistent anonymity view". Não quebra consenso financeiro diretamente, mas quebra auditability e determinismo de ring verification across nodes.

---

## 4. PROBLEMA 3 — `build_ring_inline` Determinístico

```rust
let pk = if let Some(sd) = &entry.stealth_dest {
```

**Observação:** Assumindo `stealth_dest == canonical public key representation` → ring construction é determinística. Adversário com UTXO set pode reconstruir full ring graph.

**Consequência:** Reduz o sistema para "pseudo-stealth, not true indistinguishability set". Privacy é structural, not entropy-based.

---

## 5. PROBLEMA 4 — Range Proof Check Incompleto

```rust
if !proof.verify(&Commitment(comm_pt)) {
```

**Missing invariant:** Não valida cross-output balance consistency, nem `commitment sum == input sum` (Pedersen-style invariant).

**Consequência:** Mesmo com valid range proofs, inflation detection depende APENAS do explicit scalar sum check anterior (`if input_amount < output_amount`). Assume plaintext amount availability. Quebra em fully private mode extension.

---

## 6. PROBLEMA 5 — Atomicidade Não Rollback-Safe

`spend_transaction_inputs_with_diff(...)` trackeia:
- `consumed` BEFORE mutation ✅
- `created` AFTER mutation ✅

**Risco:** Se panic ou erro ocorre mid-loop: utxo partially mutated, diff partially populated. Logical atomicity ≠ execution atomicity. Depende de caller discipline, não de type system guarantee.

---

## 7. PROBLEMA 6 — Supply Accounting Assimétrica sob Reorg

- `sub_from_supply()`
- `add_coinbase_supply()`

**Problema:** `supply_delta` só trackeia coinbase side cleanly. Transaction-level effects são implícitos.

**Risco:** Em multi-fork stress test, supply reconciliation depende inteiramente de diff correctness. **Nenhum invariant checker existe:** `assert!(sum_utxos == total_supply)`.

---

## 8. Design Strength (Positivo)

- ✅ MLSAG integrated into UTXO engine (raro)
- ✅ Explicit reorg diff model (boa arquitetura)
- ✅ Orphan resolution mechanism
- ✅ Deterministic tx hashing model
- ✅ Separação de validation, state transition, reorg execution

**Nível:** "Research prototype blockchain core", não toy chain.

---

*Audit in progress — structural analysis of state.rs + privacy.rs + reorg.rs by Gustavo. 6 problems identified so far.*
