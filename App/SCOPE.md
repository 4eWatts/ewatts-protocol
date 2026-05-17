# eWatts App Ecosystem — Escopo do Projeto

## 1. Visão Geral

Dois aplicativos que transformam o eWatts de protocolo em produto:

| App | Plataforma | Função |
|-----|-----------|--------|
| **eWatts Miner** | Windows / macOS / Linux | Minerador silencioso em background, system tray |
| **eWatts Wallet** | iOS / Android | Carteira, envio/recebimento via QR Code e NFC |

Ambos compartilham o mesmo core wallet (geração de chaves, assinatura MLSAG, construção de transações privadas).

---

## 2. eWatts Miner (Desktop)

### 2.1 Comportamento

- Inicia com o Windows (Auto-start)
- Fica no system tray (icone na bandeja)
- Mineira **apenas quando o PC está idle** (sem input do usuário por 5 min)
- Para de minerar imediatamente quando o usuário volta
- Mostra notificação toast quando encontra um bloco
- Consumo: ~5-15W durante mining (DRAM), 0W quando parado

### 2.2 Funcionalidades

- [ ] System tray com menu: Start/Stop Mining, Open Wallet, Exit
- [ ] Idle detection via `GetLastInputInfo` (Windows) / `CGSEventSourceFlagsState` (macOS)
- [ ] Geração automática de wallet na primeira execução
- [ ] Exibição de saldo e blocos minerados
- [ ] Atualização automática (GitHub releases)
- [ ] Logs para debug

### 2.3 Stack

| Componente | Tecnologia |
|-----------|-----------|
| Linguagem | Rust (reuso do core) |
| UI | `egui` ou `native Windows API` (system tray apenas) |
| Idle detection | `GetLastInputInfo` (Win32), `IOKit` (macOS) |
| Updates | `self_update` crate |

### 2.4 Tamanho estimado

- Binário compilado: ~8-12 MB (Rust + curve25519-dalek)
- RAM em idle: ~2 MB
- RAM minerando: DAG size (~40 MB mainnet, ~100 MB com folga)

---

## 3. eWatts Wallet (Mobile)

### 3.1 Funcionalidades

- [ ] Criação de stealth wallet (gera spend + view key)
- [ ] Recebimento via QR Code (exibe stealth address)
- [ ] Envio via QR Code (escaneia stealth address do destinatário)
- [ ] Envio via NFC (Android: HCE; iOS: CoreNFC)
- [ ] Scan de blockchain para ver saldo (RPC para node público)
- [ ] Histórico de transações
- [ ] Múltiplas contas
- [ ] Backup / restore via seed phrase (BIP39-style)

### 3.2 Fluxo de Envio via QR Code

```
Remetente:
  1. Abre app → "Enviar"
  2. Escaneia QR Code do destinatário (contém stealth address)
  3. Digita valor
  4. App monta transação privada:
     a. Gera commitment + range proof
     b. Seleciona ring members de UTXOs existentes (via API)
     c. Assina MLSAG
     d. Gera OneTimeAddress com ephemeral key
     e. Mostra QR Code da transação serializada
  5. Destinatário escaneia → envia para node → tx confirmada

OU:
  4. App envia tx diretamente para node público via HTTP
     (POST /api/submit_tx com JSON da transação)
```

### 3.3 Stack

| Componente | Tecnologia |
|-----------|-----------|
| Cross-platform | **React Native** (ou Flutter) |
| Crypto core | **Rust** compilado para ARM (iOS/Android) via `uniffi` ou `jni` |
| NFC | `react-native-nfc-manager` / CoreNFC |
| QR Code | `react-native-camera` + `react-native-qrcode-svg` |
| State | Redux Toolkit |
| Node RPC | gRPC ou REST (endpoints existentes) |

### 3.4 Bridge Rust → Mobile

A parte mais crítica. O Rust precisa compilar para:

- **Android:** `arm64-v8a` (quase todos os dispositivos modernos)
- **iOS:** `arm64` (iPhone 5s+)

Duas abordagens:

| Abordagem | Esforço | Manutenção |
|-----------|---------|-----------|
| **uniffi-rs** (Mozilla) | 2-3 dias | Baixa |
| **JNI manual** (Android) + **C FFI** (iOS) | 1 semana | Alta |

uniffi-rs gera automaticamente os bindings Kotlin (Android) e Swift (iOS) a partir de uma definição de interface. É a abordagem recomendada.

### 3.5 Módulos Rust para mobile

```rust
// Funções expostas para o app mobile:
fn generate_wallet() -> WalletData           // Cria stealth keypair
fn get_balance(wallet: &Wallet) -> u64        // Escaneia UTXOs
fn create_tx(wallet, to_addr, amount) -> Tx   // Monta tx privada
fn sign_tx(wallet, tx) -> SignedTx            // Assina MLSAG
fn verify_tx(tx) -> bool                      // Verifica tx localmente
```

O app mobile **não minera** — apenas cria, assina e verifica transações.

---

## 4. Cloud / Hard Lock

Duas filosofias:

### Opção A: Hard Lock (tudo no dispositivo)

- Chaves privadas **nunca** saem do celular
- Blockchain scan via RPC para node público
- DAG mining não existe no mobile
- Backup: seed phrase de 12/24 palavras

**Prós:** Soberania total. **Contras:** Recuperação via seed phrase (complexo para usuário comum).

### Opção B: Cloud com criptografia

- Chaves privadas são criptografadas com senha do usuário e armazenadas no servidor
- Servidor faz scan de blockchain por você
- App mobile consulta saldo via API

**Prós:** Experiência de usuário simples (login/senha). **Contras:** Confia no servidor (mas as chaves estão criptografadas).

### Opção C: Híbrido (recomendado)

- Wallet gerada no dispositivo (seed phrase)
- Chave pública (stealth address) enviada para servidor
- Servidor escaneia blockchain e notifica app quando há transação
- Para enviar, app assina localmente e envia tx pronta para o servidor broadcast

**O servidor nunca vê as chaves privadas.** Apenas auxilia no scan e broadcast.

---

## 5. Arquitetura de Rede

```
[App Mobile] ←→ [API Gateway] ←→ [eWatts Node (RPC)] ←→ [P2P Network]
                   ↑
            [Push Notification]
                   ↓
[App Mobile recebe notificação de saldo]
```

Para usuários comuns que não rodam node:

```
[App Mobile] ←→ [eWatts API Pública] ←→ [Node privado]
```

A API pública faz:
- `GET /api/balance/:address` → saldo (via scan de UTXOs)
- `POST /api/submit_tx` → broadcast (já existe)
- `GET /api/status` → altura da blockchain (já existe)

---

## 6. Ordem de Implementação

| Fase | O que | Tempo | Depende de |
|------|-------|-------|-----------|
| **1** | API pública (balance, tx lookup) | 3 dias | Nada |
| **2** | eWatts Miner Desktop (MVP) | 1-2 semanas | Nada |
| **3** | Bridge Rust → Mobile (uniffi) | 3 dias | Rust core (pronto) |
| **4** | eWatts Wallet iOS/Android (MVP) | 2-3 semanas | Bridge |
| **5** | NFC + QR Code offline tx | 1 semana | Wallet |
| **6** | Push notification server | 1 semana | API |

---

## 7. Tamanho do Projeto

| App | Arquivos | Linhas (Rust) | Linhas (UI) |
|-----|----------|---------------|------------|
| Core existente | 16 | ~3.300 | — |
| Miner Desktop | 5-8 | ~800 | ~200 (config) |
| Wallet Mobile | 20-30 | ~500 (bridge) | ~2.000 (React Native) |
| API Gateway | 5-8 | ~500 | — |

**Total estimado: ~5.000-7.000 linhas novas** sobre as 3.300 existentes.

---

## 8. Checklist de Requisitos

### Desktop Miner
- [ ] Auto-start com Windows
- [ ] System tray com icone
- [ ] Idle detection (5 min sem input)
- [ ] Pausa automática ao detectar uso
- [ ] Log de blocos minerados
- [ ] Wallet embutida
- [ ] Modo "sempre ligado" (para servidores)

### Mobile Wallet
- [ ] Geração de stealth wallet
- [ ] Seed phrase backup
- [ ] Scan de QR Code para receber endereço
- [ ] Exibir QR Code do próprio endereço
- [ ] Envio de transação privada
- [ ] NFC beam (Android HCE)
- [ ] NFC tap (iPhone para iPhone via CoreNFC)
- [ ] Scan de blockchain para saldo
- [ ] Notificação push de transação recebida
- [ ] Múltiplas contas

### Infraestrutura
- [ ] API pública de consulta
- [ ] Servidor de push notification
- [ ] node público RPC
- [ ] CI/CD para builds cross-platform

---

## 9. Decisões Pendentes

| Decisão | Opções | Recomendação |
|---------|--------|-------------|
| Framework mobile | React Native vs Flutter | **React Native** (maior ecossistema NFC) |
| Bridge Rust | uniffi vs JNI manual | **uniffi** (Mozilla, maduro) |
| Armazenamento de chaves | Cloud criptografado vs dispositivo | **Híbrido** (dispositivo + servidor auxiliar) |
| Node público | Próprio vs terceiros | **Próprio** (controle da API) |
| Push notification | Firebase vs APNs vs próprio | **Firebase** (Android + iOS via FCM) |

---

## 10. Próximos Passos

1. Revisar este escopo
2. Decidir framework mobile
3. Começar pela API pública (fase 1) + Desktop Miner (fase 2)
4. Depois bridge e mobile
