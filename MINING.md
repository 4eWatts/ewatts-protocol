# Ewatts Mining Guide

Mine a moeda que prova trabalho com **DRAM**, não com ASICs.

## O que você precisa

- Um servidor Linux ou Windows com **pelo menos 4 GB de RAM livre**
- **Rust instalado** (https://rustup.rs)
- ~10 minutos

## Instalação

```bash
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol
cargo build --release
```

A primeira compilação baixa ~100 crates e leva 2-5 minutos.

## Iniciar o node

```bash
# Inicializa o estado gênese (1M Ewatt para a fundação)
./target/release/ewatts-protocol init

# Ver o status
./target/release/ewatts-protocol info
```

Deve mostrar:
```
Ewatts Node
  Blocks: 1
  UTXOs:  1
  Supply: 100000000000
  VR:     333.33 uWh/Ewatt
```

## Minerar

```bash
# Minera um bloco (testnet DAG fixo, difficulty baixa)
./target/release/ewatts-protocol mine
```

Saída esperada:
```
Mining block #1...
  Mining (difficulty=100)...
  Solved! Nonce=12345, 4.00 GB at 50.00 GB/s in 80ms
  Block #1 mined! Hash: a1b2c3d4...
```

Cada bloco leva ~50-200ms em hardware moderno (DDR5). O DAG tem ~40 MB na testnet.

## Fazer mineração contínua

```bash
# Script simples de loop
while true; do
  ./target/release/ewatts-protocol mine
  sleep 1
done
```

No Windows (PowerShell):
```powershell
while ($true) {
  .\target\release\ewatts-protocol.exe mine
  Start-Sleep -Seconds 1
}
```

## Conectar em P2P

### Terminal 1 (seed node + mine)
```bash
./target/release/ewatts-protocol p2p /ip4/0.0.0.0/tcp/9001 --mine
```

### Terminal 2 (conecta no seed)
```bash
./target/release/ewatts-protocol p2p /ip4/0.0.0.0/tcp/9002 /ip4/SEU_IP/tcp/9001 --mine
```

Os nós propagam blocos via gossip. Quando um minera, os outros recebem.

## Dashboard

```bash
# Enquanto o node está rodando, abra outro terminal:
./target/release/ewatts-protocol dash
```

Abra http://localhost:8080 no navegador.

## Wallet

```bash
# Criar uma chave
./target/release/ewatts-protocol wallet new "minha"

# Listar chaves
./target/release/ewatts-protocol wallet list

# Ver saldo (escaneia blockchain por UTXOs suas)
./target/release/ewatts-protocol wallet balance
```

## Entendendo a mineração

### Memory-Bound Proof of Work (MBPoW)

Diferente do Bitcoin (SHA-256, ASIC) ou Ethereum (Ethash, GPU), o Ewatts é **memory-bound**: o gargalo é largura de banda da RAM, não poder computacional.

```
Benchmark aproximado (testnet):
  DDR5-4800 (PC moderno):  50-80  GB/s  → ~50ms/bloco
  DDR4-3200 (PC antigo):   20-30  GB/s  → ~150ms/bloco
  Servidor HBM2e (MI250):  1000+ GB/s   ← mais rápido, mas 10x mais caro
```

A tese: memória é o recurso mais democratizado do mundo. Todo computador tem. ASICs não têm vantagem em bandwidth de RAM porque DRAM é commodity.

### VR (Valor de Recurso)

VR = kWh gasto por Ewatt minerado. Mede a eficiência energética da rede. Quanto menor o VR, mais eficiente.

```
VR = (effective_commit) / (reward)
   ≈ (GB/s * tempo) / (Ewatt emitidos)
```

O VR de cada bloco aparece no dashboard e no `info`.

### Emission

A emissão por bloco começa em ~4 Ewatt e se ajusta conforme a potência total de mineração:

```
R = BASE_EMISSION × (Total_Effective / Historical_Avg)
```

- Se mais mineradores entram → sobe emissão
- Se mineradores saem → cai emissão
- Nos primeiros 10.000 blocos, cada minerador recebe no máximo 80% do que minerou (cap de ramp-up, o excesso é queimado)

## Simular blockchain

Para testes sem esperar blocos reais:

```bash
# Gera 10 blocos em sequência
./target/release/ewatts-protocol simulate 10
```

## Comandos completos

```
Commands:
  init                     Create genesis state
  mine                     Mine one block (testnet DAG)
  simulate <blocks>        Mine N blocks in sequence
  balance <pubkey_hex>     Show balance
  send <to_pubkey> <amt>   Send from genesis key
  keygen                   Generate keypair
  wallet                   Wallet commands:
    new [label]              Generate stealth keypair
    list                     List wallet keys
    balance                  Show balance from blockchain scan
    send <idx> <addr> <amt>  Create and broadcast private tx
    scan                     Scan blockchain for owned UTXOs
  info                     Show node status
  dash                     Start dashboard (port 8080)
  txhash                   Show current transaction hash (merkle root of pending txs)
  p2p <addr> [bootstrap]   Start P2P node
```

## Privacidade

Todas as transações são privadas por padrão:

- **Stealth addresses**: cada pagamento vai para um endereço único e irrepetível
- **Ring signatures**: assinatura esconde quem gastou entre 11 possíveis
- **Confidential amounts**: valor fica oculto em commitments
- **Range proofs**: prova que o valor é positivo sem revelar o número

A blockchain esconde origem, destino e valor. Apenas as partes envolvidas sabem os detalhes.

## Perguntas frequentes

**Precisa de GPU?** Não. O mining é memory-bound, CPU já basta. GPU não ajuda.

**Quanto posso minerar?** Na testnet, ~4 Ewatt por bloco a cada ~100ms ≈ 40 Ewatt/segundo. Na mainnet, o bloco leva 600 segundos.

**Isso vale dinheiro?** Não. É testnet. Ewatt não tem valor. Se um dia virar mainnet, o valor será descoberto pelo mercado (VR ~ custo de energia).

**Por que "Ewatts"?** Energy Watts — o VR mede energia por moeda.

## Links

- **Whitepaper**: https://github.com/4Ewatts/ewatts-protocol (Ewatts_Protocol_v27.md)
- **Spec**: (ewatts_v7_spec.md)
- **Dashboard**: http://localhost:8080 (quando o node estiver rodando)
