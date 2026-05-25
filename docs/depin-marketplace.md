# DePIN sobre eWatts — Marketplace de processamento útil

## Conceito

Um marketplace aberto onde qualquer nó com capacidade ociosa (CPU, GPU, RAM, banda) pode oferecer processamento e receber pagamento em Ewatt. O comprador publica uma demanda, o vendedor executa e prova, o ledger liquida.

O diferencial: o Ewatt não é só um token qualquer. O custo de emissão é conhecido e estável (VR), o que dá ao comprador previsibilidade de custo e ao vendedor uma moeda que não depende de especulação para valer algo.

## Arquitetura em 5 etapas

### Etapa 1 — Pagamento direto em Ewatt (já possível)
Qualquer transferência de Ewatt entre carteiras é um pagamento. Não precisa de smart contract. Uma exchange bilateral funciona com confiança.

### Etapa 2 — Escrow atômico (HTLC adaptado)
Comprador deposita Ewatt num output condicional. Vendedor entrega trabalho e prova. Se o comprador não liberar em N blocos, o vendedor prova entrega e saca. Resolve o problema de confiança sem ZKP.

### Etapa 3 — Proof de bandwidth (eWatts commitment)
Antes de oferecer serviço, o nó precisa minerar um commitment que prova bandwidth real. Isso filtra bots e nós de baixa qualidade. O mesmo mecanismo que ancora o custo de emissão do Ewatt serve como onboarding do vendedor.

### Etapa 4 — Prova de trabalho específica
Cada workload define sua própria prova:
- Inferência: assinatura sobre o output + hash do input
- Render: checksums de frames em intervalos aleatórios
- Tradução em lote: amostras verificadas por um modelo menor
O protocolo não entende o trabalho — só verifica que a prova corresponde ao hash combinado no momento da ordem.

### Etapa 5 — Mercado descentralizado
Ordens de compra e venda publicadas on-chain. Matching peer-to-peer. Liquidação atômica. Reputação baseada em histórico de entregas.

## Por que Ewatt?

- Custo de emissão conhecido (VR em kWh/Ewatt)
- Sem inflação arbitrária (emission rate escala com commitment real)
- O vendedor sabe exatamente quanto custou minerar o Ewatt que vai receber
- O comprador sabe que a moeda não vai ser diluída por decisão de governança

## Roadmap

| Etapa | Depende de | Status |
|-------|-----------|--------|
| 1. Pagamento direto | Mainnet Ewatt | Futuro |
| 2. Escrow atômico | Tx condicional na chain | Futuro |
| 3. Proof de bandwidth | Commitment existente | Pronto |
| 4. Prova de trabalho | Implementação por workload | Futuro |
| 5. Marketplace | Etapas 1-4 + adoção | Futuro |

---

*Documento conceitual. Nenhum código foi escrito ainda. A prioridade é mainnet Ewatt estável.*
