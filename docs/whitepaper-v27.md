# Ewatts Protocol v27 — Final (Deprecated)

**DRAM-Bound Proof-of-Energy — Whitepaper**
*May 2026 — Superseded by v28*

**See [whitepaper-v28.md](whitepaper-v28.md) for the current specification.**

**Ewatts is not a store of value. It is a ruler.**

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| v17-v22 | Pre-May 2026 | Original single-chain, single-hash MBPoW |
| v23 | 16 May 2026 | Bandwidth commitment model, VR, bootstrap mechanics |
| v24 | 17 May 2026 | Dual-chain (L1 + L2 bridge) — DEPRECATED |
| v25 | 17 May 2026 | Single-chain, dual-hash |
| v26 | 17 May 2026 | Privacy by default, selective disclosure, J_GB risk, L2 future, quantum |
| **v27** | **17 May 2026** | **Formula final: R_min=0.05×, R_max=20×, Historical_Avg=30d, RampUpFactor 80% cap, coinbase_burn. Founder time-locks on-chain. Chave pública na gênese. P2PKH addresses. Captura institucional (hard fork 95%). Framework: fidelidade de sinal, não estabilidade de preço. Selective disclosure removido.** |

---

## 1. Tese Central

Ewatts não é instrumento de hedge. Não é settlement rail para B2B. Não é SDR alternativo. É uma moeda cujo lastro está em trabalho físico verificável — bandwidth × energia — em vez de dívida soberana ou política monetária discricionária.

O sistema atual aloca capital ineficientemente porque o sinal de preço está corrompido por juros artificiais, rent-seeking e expansão sintética de ativos. Quando o medium of exchange perde conexão com produção real, capital flui para quem captura sinal monetário, não para quem cria valor real.

Ewatts tenta restaurar o sinal. A emissão não ocorre porque alguém pode (fiat); ocorre porque alguém moveu joules através de hardware real. Essa é a única forma de issuance que não pode ser inflada por decreto político.

---

## 2. Fórmula de Emissão (Final)

```
R(block) = BASE_EMISSION × (Total_Effective_Commitment / Historical_Avg_Commitment)
           clamped to [BASE × 0.05, BASE × 20]
           × RampUpFactor(block) se block < 10.000
```

### Parâmetros

| Parâmetro | Valor | Justificativa |
|-----------|-------|---------------|
| BASE_EMISSION | 100 Ewatt/bloco | Escala da emissão |
| Total_Effective_Commitment | Σ effective_commit_i (com η) | Trabalho verificado, não reivindicado |
| Historical_Avg | 4.300 blocos (30 dias) | Resistência a manipulação coordenada |
| R_min | 5 Ewatt/bloco (BASE × 0,05) | Protege democracia de acesso. Impede morte da rede em choque extremo |
| R_max | 2.000 Ewatt/bloco (BASE × 20) | Impede captura desproporcional em spikes legítimos |
| RampUpFactor | Cap 80% por miner, excesso queimado | Alinha código com filosofia. Temporário (primeiros 10.000 blocos) |

### Distribuição

```
Reward_i = (effective_commit_i / Σ effective_commit_j) × R(block)

Durante ramp-up (block < 10.000):
  se Reward_i / R(block) > 0.80:
    Reward_i = 0.80 × R(block)
    excess → coinbase_burn
```

### Founder Mining

O founder minera os primeiros blocos com hardware real (DDR5 server, ~$5.000 + ~$300/mês). Cada Ewatt que o founder detém exigiu bandwidth real e energia real.

**Time-locks on-chain:** Cada output coinbase minerado antes do bloco 10.000 carrega `spendable_after = max(50000, current_block + 40000)`. O founder não pode vender antes de ~280 dias. Isso é enforcement criptográfico, não social.

**Chave pública na gênese:** A chave de mineração do founder é publicada antes do bloco 1. Qualquer pessoa pode verificar quantos Ewatt o founder minerou e quando vender. Transparência criptográfica.

**Cap de 80% durante ramp-up:** Se o founder (ou qualquer miner) está sozinho, 20% do reward por bloco é queimado. O incentivo é convidar outros miners para a rede, não capturar tudo sozinho.

---

## 3. Autobalanceamento — Consequência Emergente

O protocolo **não tem mecanismo de estabilização de preço**. A convergência ao custo marginal de eletricidade é consequência emergente de:

1. **Emissão linear honesta:** Cada Ewatt exige trabalho real. Sem damping artificial.
2. **VR transparente:** o custo de produção médio (kWh/Ewatt) é on-chain e público.
3. **Mineração competitiva:** se preço de mercado > custo de produção, miners entram. Se preço < custo, miners saem.

Volatilidade de preço em torno do custo marginal **não é um problema que o protocolo precisa resolver.** É o comportamento esperado de um mercado que descobre preço honestamente. Bitcoin oscila 50-80% em torno do custo de produção; Ewatts pode oscilar mais ou menos — ambas são aceitáveis sob a tese de fidelidade de sinal.

---

## 4. Dual-Hash Architecture

### 4.1 Mining Hash (MBPoW, 600s)

Unchanged from v23. Full MBPoW DAG walk:
- Bandwidth commitment declaration and verification
- Efficiency checks (η penalty/cap)
- Reward distribution via formula above

### 4.2 Transaction Hash (Fast, <3ms)

Lightweight verification between mining blocks. Ring signature verification, UTXO existence, nonce uniqueness. Pre-confirmation only — finality comes from the mining hash.

---

## 5. Privacidade

### 5.1 Intocável

Privacidade é uma propriedade do protocolo, não uma configuração. Ring signatures, stealth addresses, confidential amounts — todos ativos desde o bloco de gênese. Sem exceção.

### 5.2 Proteção Contra Captura Institucional

O maior risco à privacidade do Ewatts não é criptográfico — é institucional. Um governo não precisa quebrar Ed25519. Precisa apenas de um desenvolvedor que aceite incluir um compliance update.

**Defesa:** Qualquer alteração no nível de privacidade (redução de mixins, introdução de disclosure obrigatório, remoção de stealth addresses, enfraquecimento de confidential amounts) requer **hard fork com supermaioria de 95% dos miners E 95% dos full nodes**.

Soft fork é compressível por um maintainer. Hard fork exige que a comunidade escolha. Não há upgrade silencioso.

### 5.3 P2PKH-Style Addresses

Toda chave pública é armazenada como hash na blockchain, não como chave direta. A chave real só é revelada no momento de gastar o UTXO. Isso protege contra roubo quântico retroativo: um atacante quântico não tem a chave pública para atacar até que a transação seja transmitida — janela de minutos, não de anos.

Implementado desde o bloco de gênese. Custo: ~20 bytes (hash de 20 bits em vez de chave de 32 bits — economiza 12 bytes).

---

## 6. J_GB — Parâmetro de Protocolo

### 6.1 Fixo na Gênese

J_GB = 0,08 J/GB, calibrado para DDR5.

### 6.2 Recalibração via Hard Fork

A cada 2-3 anos, a comunidade pode auditar empiricamente o J_GB efetivo medindo consumo de hardware representativo, propor um novo valor, e ativar via hard fork explícito.

O J_GB não é uma constante física pura — é um parâmetro de protocolo. A calibração periódica via consenso social é mais honesta que auto-ajuste algoritmico.

### 6.3 Equipamentos

| Hardware | GB/s | J/GB | Custo | $/GB/s |
|----------|------|------|-------|--------|
| DDR5 (server, 8 DIMMs) | ~400 | 0,08 | ~$5.000 | $12,50 |
| H100 GPU | ~800 | 0,15 | ~$25.000 | $31,25 |
| H200 GPU | ~3.350 | 0,15 | ~$30.000 | $8,96 |
| RTX 5090 | ~512 | 0,12 | ~$2.000 | $3,91 |

---

## 7. Quantum Computing

### 7.1 Threat Assessment

| Primitiva | Algoritmo | Risco | Timeline |
|-----------|-----------|-------|----------|
| Ring signature | Ed25519 (curva) | **Crítico** — quebra com Shor | 2030-2035 |
| Stealth address | Ed25519 | **Crítico** | 2030-2035 |
| Pedersen commitment | Ed25519 | **Crítico** | 2030-2035 |
| MBPoW (DAG walk) | Keccak-256/SHA-512 | **Baixo** — Grover só acelera 2× | 2035+ |

### 7.2 Migração em 3 Fases

1. **Gênese (2026):** Ed25519 + P2PKH-style addresses. TX ~2,8KB. UTXOs protegidos contra roubo quântico retroativo.
2. **2028-2030:** Soft fork permite transações FALCON (666 B/sig) como alternativa.
3. **2032+:** Ed25519 desativado. Todas as transações obrigatoriamente FALCON. TX ~11KB.

---

## 8. L2 — Futuro

L2 development é post-launch. Interface documentada na spec. Não implementado no v27.

---

## 9. Known Risks

| Risco | Severidade | Mitigação |
|-------|-----------|-----------|
| Atração de miners | Alta (social) | Founder mining + cap 80%. Estratégia de comunicação focada em universidades, Monero community, data centers renováveis |
| Concentração DRAM | Média (3 fabricantes) | Sem defesa protocol-level |
| J_GB drift | Média | Recalibração via hard fork periódico |
| Quantum computing | Média | P2PKH + FALCON migration |
| Captura institucional | Média | Hard fork 95% para mudanças de privacidade |
| Cartel pump-and-dump | Baixa | Janela de 30 dias para Historical_Avg |

---

*Ewatts Protocol v27 — May 2026*
