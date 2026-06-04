# eWatts Mining Guide

Mine a moeda que prova trabalho com **DRAM**, não com ASICs. Qualquer computador com 4 GB de RAM pode minerar.

> ⚠️ **Espaço em disco:** A compilação do eWatts consome ~1,5 GB. A blockchain (testnet) ocupa menos de 50 MB. Certifique-se de ter pelo menos 2 GB livres.

## Rápido (usuário Windows)

Abra o **PowerShell** (botão direito no Menu Iniciar → Windows PowerShell):

```
cd C:\
mkdir ewatts
cd ewatts
```

Cole os comandos abaixo UM DE CADA VEZ, esperando cada um terminar:

**Passo 1 — Instalar Rust**
```
curl.exe --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
Aperta Enter nas opções padrão. Leva ~1 minuto.

**Passo 2 — Fechar e reabrir o PowerShell** (pra carregar o Rust)

**Passo 3 — Baixar e compilar o eWatts** (6 minutos)
```
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol
cargo build --release
```
> ⚠️ **Download de 150 MB de dependências.** Pode usar ~1,5 GB de espaço temporário. Internet estável recomendada.

**Passo 4 — Conectar na testnet e minerar**
```
.\target\release\ewatts-protocol start --p2p --p2p-port 9001 --dash-port 8080 --difficulty 1 --bootstrap /ip4/178.104.193.50/tcp/25080/p2p/12D3KooWDTpEvdP2FneHTxRSAhLLykRm5yRMff7S9v3Pvtz2RUf1
```
Aparecerá:
```
Dashboard: http://0.0.0.0:8080/
P2P Node ID: 12D3KooW...
Genesis: 1,000,000 Ewatt to <endereço>
P2P: Dialing bootstrap...
```

**Passo 5 — Ver o dashboard**
Abra http://localhost:8080 no navegador.

**Passo 6 — Parar de minerar**
Aperta Ctrl+C no PowerShell.

---

## Instalação padrão (Linux/Mac)

```bash
# Instalar Rust (se não tiver)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env

# Baixar e compilar
git clone https://github.com/4Ewatts/ewatts-protocol.git
cd ewatts-protocol
cargo build --release

# Conectar na testnet
./target/release/ewatts-protocol start --p2p --p2p-port 9001 --dash-port 8080 --difficulty 1 --bootstrap /ip4/178.104.193.50/tcp/25080/p2p/12D3KooWDTpEvdP2FneHTxRSAhLLykRm5yRMff7S9v3Pvtz2RUf1
```

---

## O que cada comando faz

| Comando | O que acontece | Quanto tempo |
|---------|---------------|--------------|
| `git clone` | Baixa o código do eWatts | ~10s |
| `cargo build --release` | Compila o programa | 2-6 minutos |
| `start --p2p ...` | Inicia o node e minera | Instantâneo |

---

## Problemas comuns

**"bind address already in use"**
O dashboard já está rodando em outra janela. Mude a porta:
```
--dash-port 8081
```

**"connection refused" / bootstrap não conecta**
Servidor pode estar em manutenção. Espere 1 minuto e tente de novo.

**Antivírus bloqueando**
Adicione a pasta `ewatts-protocol\target\release\` como exceção no antivírus.

**PowerShell não reconhece `git`**
Instale o Git for Windows: https://git-scm.com/download/win (opções padrão).

**Disco cheio durante compilação**
A compilação usa ~1,5 GB temporários. O `cargo build` ocupa espaço em `%USERPROFILE%\.cargo\` e na pasta `target\`. Limpe com:
```
cargo clean
```

---

## Perguntas frequentes

**Precisa de GPU?** Não. O mining é memory-bound. Só RAM importa.

**Quanto vou minerar?** Na testnet, ~214 eWatt por bloco. Na mainnet, o bloco leva 600 segundos. Testnet não tem valor real.

**Isso vale dinheiro?** Não. Testnet. Os tokens não têm valor. É um experimento.

**Precisa deixar o PC ligado 24h?** Não. Roda quando você quiser. Fechou a janela, parou.

**Tem versão mais fácil?** Ainda não. O installer automático (`curl -sSf https://ewatts.org/install.sh`) só funciona em Linux/Mac. Windows exige PowerShell.

---

## Ver seu saldo

Com o node rodando, abra OUTRO PowerShell:

```
cd D:\ewatts\ewatts-protocol
.\target\release\ewatts-protocol wallet balance
```

## Enviar transação

```
.\target\release\ewatts-protocol wallet send 0 <endereco_hex> <quantidade>
```

---

## Links

- **Site:** https://ewatts.org
- **Dashboard público:** http://178.104.193.50:8080
- **Código:** https://github.com/4Ewatts/ewatts-protocol
