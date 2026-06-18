# Ewatts — 3-Layer Architecture (Formal Spec)

> Core Principle: O sistema é dividido em três camadas não misturáveis, com separação rígida de:
> - **execução** (engineering reality)
> - **interpretação** (economic hypothesis)
> - **validação adversarial** (system robustness)

---

## LAYER 1 — PROTOCOL CORE (Deterministic State Machine)

**Objective:** Definir um sistema único de execução determinística que qualquer nó pode verificar independentemente.

### 1.1 State Model

O sistema é uma máquina de estados:

```
S_{t+1} = f(S_t, B_t)
```

Onde:
- `S_t`: estado global (UTXO + commitments + metadata)
- `B_t`: bloco válido
- `f`: função determinística de transição

### 1.2 Block Validity Rules

Um bloco é válido se:
1. Proof-of-work satisfaz threshold definido pelo difficulty function
2. Estado anterior existe
3. Transição de UTXO é consistente
4. Commitments são verificáveis
5. Não viola invariantes de emissão

### 1.3 Emission Function (u64 deterministic model)

- Emissão é função do estado + altura
- Sem input externo
- Sem discretionary adjustment
- `E_t = g(height, state)`

### 1.4 Consensus Rule (minimal definition)

Em ausência de rede: a cadeia válida é aquela com maior accumulated work (ou equivalent metric definida na Layer 2).

### 1.5 Security Invariants

- No double spend
- No inflation beyond schedule
- Deterministic replay
- State convergence under valid propagation

**Output of Layer 1:** single canonical ledger rule, deterministic validation engine, cryptographic correctness.

---

## LAYER 2 — ECONOMIC HYPOTHESIS LAYER (Interpretation Layer)

**Objective:** Definir o significado econômico do sistema sem interferir no protocolo.

### 2.1 Core Hypothesis

Monetary systems anchored in physical computation substrates exhibit different long-term stability properties depending on the improvement rate of their underlying physical constraints.

### 2.2 Substrate Classes

**Class A — Energy-bound systems (ASIC-like)**
- cost ≈ energy consumption
- fast efficiency improvement
- competitive hardware arms race

**Class B — Memory-bound systems (DRAM-latency-like)**
- cost ≈ memory access + latency physics
- slower improvement curve (~1–2% annual)
- more stable cost structure

### 2.3 Key Variable: Improvement Rate Differential

Define:
```
Δ = rate(substrate improvement)
```

- ASIC: high Δ
- DRAM latency: low Δ

**Hypothesis:** lower Δ → more stable monetary emission regime

### 2.4 Falsifiability Conditions

The hypothesis is false if:
1. Substrate advantage converges quickly (Δ becomes irrelevant)
2. System becomes economically dominated by secondary abstraction layer
3. Cost of attack decouples from physical constraint

### 2.5 What this layer does NOT do

- Does NOT define consensus rules
- Does NOT define validation logic
- Does NOT affect protocol execution

**Output of Layer 2:** interpretative economic framework, measurable hypothesis space, comparison model between monetary substrates.

---

## LAYER 3 — ADVERSARIAL CONSENSUS LAYER (Robustness Engine)

**Objective:** Model system behavior under strategic, noisy, and adversarial conditions.

### 3.1 Threat Model

Nodes may behave as:
- honest
- delayed
- partitioned
- selfish
- conflicting-state
- withholding-capable

### 3.2 Network Model

System operates under:
- latency variance
- message duplication
- message reordering
- partial partitions
- probabilistic loss (optional)

### 3.3 Fork Reality Model

Unlike Layer 1 assumption, here: multiple competing valid chains can exist simultaneously.

### 3.4 Fork Choice Rule (abstract definition)

Each node selects chain C maximizing:

```
Score(C) = W(C) - C_attack(C)
```

Where:
- `W(C)`: accumulated work or equivalent substrate-weighted metric
- `C_attack(C)`: cost-adjusted adversarial penalty model (defined per experiment)

### 3.5 Adversarial Objectives

Attackers may attempt:
- maximize reorg probability
- maximize stale block rate
- induce divergence between nodes
- extract value via timing asymmetries

### 3.6 System-Level Properties Tested

- convergence under chaos
- safety under partition
- stability under adversarial delay
- robustness of fork selection rule

**Output of Layer 3:** emergent consensus behavior, adversarial resilience metrics, system stability envelope.

---

## Module-to-Layer Mapping

| Módulo | Layer | O que valida |
|---|---|---|
| `smoke.rs` | Layer 1 | Pipeline correctness, round-robin mining |
| `tests.rs` integração | Layer 1 | Private tx, founder lock, double-spend, reorg |
| `shuffle.rs` | Layer 3 | Block propagation under network noise |
| `reorg.rs` | Layer 1 | Fork resolution engine |
| `commitment.rs` | Layer 1 | Bandwidth commitment validation |
| `reward.rs` | Layer 1+2 | Emission math (Layer 1) + economic interpretation (Layer 2) |
| (futuro) adversarial.rs | Layer 3 | Strategic miner competition |

---

## CROSS-LAYER SEPARATION RULE (CRITICAL)

| Layer | Can influence protocol? | Role |
|---|---|---|
| Layer 1 | **YES** | execution truth |
| Layer 2 | **NO** | interpretation only |
| Layer 3 | **NO** | testing environment only |

## SYSTEM SUMMARY (ONE-LINER)

> Ewatts is a deterministic monetary state machine (Layer 1), evaluated under physical substrate hypotheses (Layer 2), and stress-tested in adversarial distributed environments (Layer 3).

## WHY THIS STRUCTURE MATTERS

Sem essa separação:
- economia invade protocolo
- simulação vira verdade
- narrativa vira regra de consenso

Com essa separação:
- protocolo é verificável
- hipótese é falsificável
- adversarial layer é experimental

---

# LAYER 3 — FORMAL ADVERSARIAL MODEL (v1.0)

> Definição matemática de sistema distribuído sob adversário econômico + network stochasticity.

## 0. Objective

Definir um sistema onde:
- múltiplos nós observam estados inconsistentes
- mensagens sofrem atraso, duplicação e perda
- adversários podem explorar timing e fork space
- ainda assim existe convergência probabilística ou determinística

## 1. Network Model

### 1.1 Graph Definition

Sistema é um grafo dinâmico:

```
G_t = (V, E_t)
```

- `V`: nós honestos + adversariais
- `E_t`: conectividade estocástica no tempo

### 1.2 Message Delivery Function

Cada mensagem `m` enviada em tempo `t`:

```
D(m, t) ~ P(τ, δ, ρ)
```

Onde:
- `τ`: delay distribution (latência)
- `δ`: duplication probability
- `ρ`: drop probability

### 1.3 Adversarial Control

Adversário controla:
- scheduling de mensagens
- subset de delays
- selective propagation of blocks
- fork visibility manipulation

**Constraint:** adversary não quebra criptografia, apenas timing e topology.

## 2. Blockchain State Space

### 2.1 Global State

Cada nó mantém:

```
S_i^t = (C_i^t, U_i^t)
```

- `C_i^t`: chain view (DAG parcial ou linear)
- `U_i^t`: UTXO set local

### 2.2 Valid State Set

Define:

```
S = {S : valid_transition(S)}
```

### 2.3 Transition Function

```
S_{t+1}^i = f(S_t^i, M_t^i)
```

Onde `M_t^i` = mensagens recebidas no nó i.

## 3. Fork Space Model

### 3.1 Fork Set

Em vez de uma única chain:

```
F_t = {C_1, C_2, ..., C_n}
```

Cada fork tem:
- work accumulated
- propagation delay
- local visibility set

### 3.2 Fork Weight Function

Definimos peso:

```
W(C) = Σ_{b ∈ C} w(b)
```

### 3.3 Network Adjusted Score

```
S(C) = W(C) - λ · D(C)
```

Onde:
- `D(C)`: delay penalty (propagation disadvantage)
- `λ`: sensitivity parameter

## 4. Fork Choice Rule (Core Contribution)

Cada nó escolhe:

```
C* = argmax_{C ∈ F_t} S(C)
```

**Interpretation:** Isso transforma consenso em **otimização sob informação parcial + atraso estocástico**.

## 5. Adversarial Objectives

Adversário tenta maximizar:

- **(A) Reorg probability:** `P(reorg)`
- **(B) Stale rate:** `R_stale = orphan_blocks / total_blocks`
- **(C) State divergence:** `ΔS = max_{i,j} distance(S_i, S_j)`

## 6. System Invariants (CRITICAL)

**I1 — Safety (No double spend)**

```
∀ i,t: U_i^t is consistent under valid transitions
```

**I2 — Bounded divergence**

```
E[ΔS] < ε
```

**I3 — Convergence under finite delay**

Se:
- network eventual connectivity holds
- adversary does not censor indefinitely

Então:

```
lim_{t→∞} C_i^t = C_j^t
```

**I4 — Work monotonicity**

```
W(C_{t+1}) ≥ W(C_t)
```

## 7. Failure Conditions (ESSENTIAL FOR PAPER)

Sistema falha se qualquer um ocorrer:

- **F1 — Permanent partition:** `G_t → disconnected components`
- **F2 — Delay domination attack:** `D(C_honest) >> D(C_adversary)` → adversário controla fork selection sem mais hashpower
- **F3 — State desynchronization explosion:** `lim ΔS → ∞`
- **F4 — Incentive inversion:** se `cost_to_produce < reward_signal` sem correção estrutural → inflação implícita do sistema

## 8. Key Insight

**Bitcoin model clássico:** adversário compete por hashpower.

**Aqui:** adversário compete por **informação temporal + visibility topology**.

Isso é mais próximo de:
- real-world distributed markets
- high-frequency settlement systems
- fragmented monetary regimes

## 9. Experimental Mapping

Seu código atual já implementa:
- ✅ delay stochasticity
- ✅ duplication
- ✅ partial convergence
- ✅ state divergence check

O que falta para fechar Layer 3 formal:
1. Explicit adversarial scheduler model
2. Fork-choice scoring function implemented in simulation
3. Measurable ΔS metric (formal distance function)
4. Failure condition triggers (F1–F4 instrumented)

## 10. One-Line Formal Definition

> Ewatts is a monetary state machine whose fork-choice rule optimizes for network-adjusted work under partial information, with adversarial convergence bounded by delay topology.
