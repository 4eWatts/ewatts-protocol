# Audit: reorg.rs — Fork-choice + Reorg Engine

> **Auditor:** Gustavo
> **Date:** 23 May 2026
> **Type:** Consensus-level audit
> **Module:** `src/reorg.rs`

## TL;DR (Veredito)

O design está conceitualmente forte, mas ainda não é "consensus-grade" por 4 motivos estruturais:
1. Fork-choice mistura lógica local + global sem normalização formal
2. Reorg depende de grafos implícitos no ChainStore (não explicitamente verificados aqui)
3. `to_apply`/`to_unwind` são derivados sem prova de consistência total
4. Estado (UTXO) e cadeia não são atomicamente acoplados no nível de decisão

**Status:** "Correto na prática esperada", não "adversarialmente fechado".

---

## 1. Fork-Choice: O Modelo Atual

```rust
let new_work = parent_work + block_work;
if new_work > current_work => reorg
```

**Problema oculto:** Assume que "o trabalho acumulado define a árvore de consenso", mas `work_at(&prev_hash)` pode estar:
- não existente ainda
- desatualizado em sidechains
- inconsistente com `chain_tip_work()`

**Risco:** Fork decision baseada em estado incompleto do grafo.

---

## 2. LCA é Frágil

```rust
let lca = store.find_lca(&prev_hash, &tip_hash);
```

**Problema:** Assume que store contém histórico completo, graph is fully connected, no missing parent paths.

Mas no sistema real:
- orphans existem
- blocks podem chegar fora de ordem
- sidechains podem não estar fully expanded

**Risco:** LCA pode ser localmente correto mas globalmente errado.

---

## 3. `to_apply` Construção — Problema Crítico

```rust
let mut to_apply = store.get_chain_to_fork(&prev_hash, &fork_point);
to_apply.reverse();
to_apply.push(hash);
```

**Problema estrutural:** Monta a cadeia sem:
- validar continuidade hash→parent
- recalcular cumulative work do conjunto inteiro
- validar se chain remains valid após inserção

**Assunção implícita:** "Se cada bloco individual é válido, a cadeia é válida". Isso NÃO é suficiente sob reorg adversarial.

---

## 4. Snapshot Rollback Mask

```rust
let state_snapshot = state.clone();
let store_snapshot = store.clone();
```

**Problema:** Correto para testnet, inviável para mainnet-scale:
- O(n) clone de UTXO set
- O(n) clone de ChainStore
- memory amplification linear com fork depth

**Risco:** Mascara bugs reais de partial reorg failure, race conditions, inconsistent diff application.

---

## 5. CRÍTICO: Inconsistency Window

Durante reorg:
```rust
state.unwind_with_diff(diff)?
state.apply_block_and_track(...)
store.set_chain_tip(...)
```

**Problema:** Não existe:
- atomic commit boundary
- intermediate consistency lock
- deterministic rollback guarantee per-step

**Risco:** UTXO já unwinded, mas apply falha no meio. Mesmo com snapshot, é "all-or-nothing after fact", não transactional.

---

## 6. Resurrect Logic (Subestimado)

```rust
let all_still_spent = tx.inputs.iter()
    .all(|i| state.spent_key_images().contains(&i.key_image));
```

**Problema:** Depende de state AFTER reorg, mas before mempool reconciliation.

**Risco:** False resurrection, phantom replays, inconsistent mempool state vs chain state.

---

## O Que Está Muito Bom

- ✅ Fork-choice é simples e auditable (sem overengineering)
- ✅ Separação entre `analyze_fork` (pure decision) e `execute_reorg` (state mutation)
- ✅ Explicit reorg plan abstraction (`to_unwind` / `to_apply`)

---

## Veredito Final

**Engenharia:** 7.5/10 — sólido, bem estruturado, gaps conhecidos
**Adversarial readiness:** 4/10 — fecha para cenários esperados, vulnerável a edge cases
**Mainnet readiness:** 3/10 — snapshot clone não escala, falta atomicidade transactional

---

*Audit completed 23 May 2026 by Gustavo. 6 structural problems identified + 3 scores.*
