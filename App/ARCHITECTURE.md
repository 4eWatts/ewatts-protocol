# eWatts — Arquitetura de Software

## Índice
1. [Desktop Miner (eWatts Miner)](#1-desktop-miner)
2. [Mobile Wallet (eWatts Wallet)](#2-mobile-wallet)
3. [Core Compartilhado](#3-core-compartilhado)
4. [Infraestrutura de Rede](#4-infraestrutura-de-rede)
5. [Segurança](#5-seguranca)

---

## 1. Desktop Miner

### 1.1 Visão Arquitetural

```
┌─────────────────────────────────────────────────────────┐
│                   eWatts Miner                          │
├─────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────┐  │
│  │               System Tray UI (egui)              │  │
│  │  [Status: Mining/Idle] [Blocks: 47] [Saldo: X]  │  │
│  └──────────────────────┬───────────────────────────┘  │
│                         │                              │
│  ┌──────────────────────▼───────────────────────────┐  │
│  │                 App Controller                    │  │
│  │  ┌──────────┐  ┌──────────┐  ┌────────────────┐  │  │
│  │  │  Wallet  │  │  Miner   │  │  Idle Detector  │  │  │
│  │  │  Manager │  │  Engine  │  │  (Win32 API)    │  │  │
│  │  └────┬─────┘  └────┬─────┘  └───────┬────────┘  │  │
│  └───────┼──────────────┼────────────────┼───────────┘  │
└──────────┼──────────────┼────────────────┼──────────────┘
           │              │                │
     ┌─────▼──────────────▼────────────────▼──────────┐
     │               Core Rust Library                │
     │  ┌──────────┐ ┌──────────┐ ┌────────────────┐  │
     │  │  Block   │ │  MLSAG   │ │  MBPoW DAG     │  │
     │  │  Chain   │ │  Privacy │ │  Mining        │  │
     │  └──────────┘ └──────────┘ └────────────────┘  │
     └────────────────────────────────────────────────┘
```

### 1.2 Componentes

#### App Controller
- Gerencia ciclo de vida do app
- Coordena wallet, miner e idle detector
- Expõe API local via IPC (Unix socket no Linux, named pipe no Windows)

#### Wallet Manager
- Gera stealth keypair na primeira execução
- Salva chaves em disco criptografado (AES-256-GCM com senha derivada do usuário)
- Escaneia blockchain periodicamente via RPC para atualizar saldo

#### Miner Engine
- Thread dedicada de mining
- Carrega DAG na memória (40 MB testnet, ~40 GB mainnet)
- Executa loop: mine → submit → next
- Reporta progresso para App Controller via channel

#### Idle Detector
- Windows: `GetLastInputInfo()` + `WaitForInputIdle()`
- macOS: `CGEventSourceFlagsState(kCGEventSourceStateHIDSystemState)`
- Linux: `xss` (X Screensaver) ou `wayland-idle-inhibit`
- Threshold: 5 minutos sem input → começa mining
- Input detectado → para mining em <500ms

### 1.3 Fluxo de Mining

```
[Idle Detector] → 5 min idle → sinaliza para App Controller
                    ↓
[App Controller] → ativa Miner Engine
                    ↓
[Miner Engine]  → carrega DAG (se necessário)
                → gera nonce
                → executa MBPoW (10k acessos à DAG)
                → verifica dificuldade
                → se passou: monta bloco + commit + coinbase
                → salva bloco + atualiza UTXO set
                → envia para peers via P2P
                → volta ao loop
                    ↓
[Idle Detector] → input detectado → sinaliza stop
                    ↓
[Miner Engine]  → para mining em <500ms
                → libera DAG (opcional)
                → volta a idle
```

### 1.4 Idle Detection (Windows)

```rust
// winapi crate
use windows::Win32::UI::Input::GetLastInputInfo;

fn check_idle() -> u64 {
    let mut last = LASTINPUTINFO::default();
    unsafe { GetLastInputInfo(&mut last) };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    now - last.dwTime as u64  // ms desde último input
}
```

### 1.5 Auto-Start

- Windows: chave no Registro `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
- macOS: `LaunchAgent` plist
- Linux: `~/.config/autostart/` desktop file

### 1.6 Build e Distribuição

```bash
# Build para Windows (cross-compile do Linux ou nativo)
cargo build --release --target x86_64-pc-windows-msvc

# Empacotar em .exe único
# Usar https://github.com/rust-cross/cargo-wix para .msi installer
```

Distribuição via **GitHub Releases** com auto-update (`self_update` crate).

---

## 2. Mobile Wallet

### 2.1 Visão Arquitetural

```
┌─────────────────────────────────────────────────────────┐
│                  eWatts Wallet (App)                     │
├─────────────────────────────────────────────────────────┤
│  ┌──────────────────────────────────────────────────┐  │
│  │              React Native UI Layer               │  │
│  │  ┌─────────┐ ┌──────────┐ ┌─────────┐ ┌──────┐  │  │
│  │  │Dashboard│ │  Send    │ │ Receive │ │History│  │  │
│  │  │ (saldo) │ │  (QR/NFC)│ │ (QR)    │ │       │  │  │
│  │  └─────────┘ └──────────┘ └─────────┘ └──────┘  │  │
│  └──────────────────────┬───────────────────────────┘  │
│                         │                              │
│  ┌──────────────────────▼───────────────────────────┐  │
│  │           Native Bridge (JSON-RPC via FFI)        │  │
│  │  ┌─────────────┐  ┌──────────┐  ┌─────────────┐  │  │
│  │  │  Key Store  │  │  Tx      │  │  Network     │  │  │
│  │  │  (Keychain/ │  │  Builder │  │  Client      │  │  │
│  │  │   Keystore) │  │          │  │  (REST)      │  │  │
│  │  └──────┬──────┘  └────┬─────┘  └──────┬──────┘  │  │
│  └─────────┼──────────────┼────────────────┼─────────┘  │
└────────────┼──────────────┼────────────────┼────────────┘
             │              │                │
       ┌─────▼──────────────▼────────────────▼──────┐
       │         Rust Core (uniffi-rs bridge)        │
       │  ┌──────────┐  ┌───────┐  ┌─────────────┐  │
       │  │  Wallet  │  │MLSAG  │  │  Pedersen    │  │
       │  │  Keys    │  │Sign   │  │  Commitments │  │
       │  └──────────┘  └───────┘  └─────────────┘  │
       └─────────────────────────────────────────────┘
```

### 2.2 Bridge Rust → Mobile (uniffi-rs)

A [uniffi-rs](https://github.com/mozilla/uniffi-rs) da Mozilla gera bindings nativas automaticamente:

```idl
// ewatts.udl — Definição de interface uniffi
namespace ewatts {
  Wallet generate_wallet();
  u64 get_balance(string address);
  Transaction create_transaction(string from_secret, string to_addr, u64 amount);
  string sign_transaction(Transaction tx, string secret_key);
  boolean verify_transaction(Transaction tx);
};

[Throwing]
dictionary Wallet {
  string spend_key;
  string view_key;
  string address;
};

dictionary Transaction {
  string hex_data;
  string tx_hash;
};
```

Gera automaticamente:
- **Android:** Kotlin bindings + JNI
- **iOS:** Swift bindings + C FFI

### 2.3 QR Code / NFC

#### Fluxo de Recebimento

```
Usuário A quer receber:
  1. Abre app → "Receber"
  2. App exibe QR Code contendo:
     { "v": 1, "addr": "3b82f6...a1b2" }  // stealth address (32 bytes hex)
  3. Usuário B escaneia com a câmera
  4. App B extrai stealth address
```

#### Fluxo de Envio Offline (QR)

```
Usuário A quer enviar para B, sem internet:
  1. A monta transação completa:
     - Commitment + range proof
     - MLSAG ring signature (ring members obtidos previamente)
     - Serializa como JSON compacto
  2. A exibe QR Code da transação
  3. B escaneia com app
  4. B salva transação no dispositivo
  5. Quando B tiver internet, envia via POST /api/submit_tx
```

#### NFC

- **Android HCE (Host Card Emulation):** app se comporta como cartão NFC
- **iOS CoreNFC:** leitura de tags NFC (iPhone como leitor)
- **Formato:** NDEF message contendo stealth address ou tx serializada

### 2.4 Armazenamento de Chaves

| Plataforma | Tecnologia |
|-----------|-----------|
| iOS | **Keychain** (Secure Enclave quando disponível) |
| Android | **Android Keystore** (TEE/StrongBox) |
| Backup | Seed phrase BIP39 (12 palavras) |

A chave privada NUNCA sai do dispositivo. A seed phrase é a única forma de backup.

### 2.5 Conexão com Node

O app mobile precisa de um node para consultar saldo e submeter transações:

1. **Node público eWatts** (rodando no servidor)
2. **Conexão REST** (endpoints já existem)
3. **API Key** para rate limiting

Endpoints necessários no node:

| Endpoint | Função | Status |
|----------|--------|--------|
| `GET /api/status` | Altura, supply, UTXOs | ✅ Existe |
| `POST /api/submit_tx` | Broadcast de transação | ✅ Existe |
| `GET /api/balance/:address` | Saldo de uma stealth address | ❌ Precisa criar |
| `GET /api/tx/:hash` | Detalhes de uma transação | ❌ Precisa criar |
| `GET /api/utxos/:address` | UTXOs de uma address | ❌ Precisa criar |
| `GET /api/ring/:amount` | Ring members para um valor | ❌ Precisa criar |

### 2.6 Notificações Push

- Firebase Cloud Messaging (FCM) para Android e iOS
- Servidor escaneia blockchain e notifica app quando nova tx chega para uma address registrada

---

## 3. Core Compartilhado

Ambos os apps compartilham o mesmo core Rust:

```
core/
├── src/
│   ├── lib.rs            → Ponto de entrada da lib
│   ├── wallet.rs         → Geração de chaves, scan UTXOs
│   ├── privacy.rs        → Stealth, MLSAG, Pedersen, range proofs
│   ├── block.rs          → Estruturas de bloco e tx
│   ├── transaction.rs    → Construção e serialização de tx
│   └── state.rs          → UTXO set, validação
├── Cargo.toml
└── src/ewatts.udl        → Definição uniffi para bindings mobile
```

Diferenças entre plataformas:

| Funcionalidade | Desktop Miner | Mobile Wallet |
|---------------|--------------|---------------|
| Mining (MBPoW) | ✅ Sim | ❌ Não |
| DAG em memória | ✅ Sim (~40 GB) | ❌ Não |
| Wallet keygen | ✅ Sim | ✅ Sim |
| Assinar tx (MLSAG) | ✅ Sim | ✅ Sim |
| Scan UTXOs | ✅ Direto (node local) | ✅ Via RPC |
| NFC | ❌ Não | ✅ Sim |
| QR Code | ❌ Não | ✅ Sim |
| Idle detection | ✅ Sim | ❌ Não |
| Auto-start | ✅ Sim | ❌ Não |

---

## 4. Infraestrutura de Rede

```
                         ┌──────────────────┐
                         │   eWatts Node    │
                         │  (servidor público)│
                         │  Porta 8080 (API) │
                         │  Porta 9001 (P2P) │
                         └────────┬─────────┘
                                  │
            ┌─────────────────────┼─────────────────────┐
            │                     │                     │
    ┌───────▼───────┐     ┌───────▼───────┐     ┌───────▼───────┐
    │ Desktop Miner │     │ Mobile Wallet │     │  Outros Nodes │
    │ (CPU idle)    │     │ (iOS/Android) │     │  (P2P)        │
    └───────────────┘     └───────────────┘     └───────────────┘
```

### 4.1 API Pública (a criar)

```rust
// Novos endpoints no dashboard HTTP
GET /api/balance/:address
  → { "address": "...", "balance": 42000000, "utxos": 3 }

GET /api/tx/:hash  
  → { "hash": "...", "inputs": [...], "outputs": [...], "block": 42 }

GET /api/ring/pool  
  → { "members": ["utxo_ref_1", ..., "utxo_ref_11"] }
  // Retorna UTXOs aleatórios para usar como ring members
```

### 4.2 Hospedagem

| Serviço | Uso |
|---------|-----|
| **Node público** | AWS EC2 / DigitalOcean (t2.medium, 8 GB RAM, ~$30/mês) |
| **API REST** | Mesmo processo do node (porta 8080) |
| **Push notification** | Servidor separado em Node.js ou Rust (Firebase Admin SDK) |
| **Site** | GitHub Pages (grátis, já configurado) |

---

## 5. Segurança

### 5.1 Ameaças e Mitigações

| Ameaça | Impacto | Mitigação |
|--------|---------|-----------|
| Vazamento de chave privada | Perda total de fundos | Keystore/Keychain + seed phrase offline |
| Malware rouba wallet file | Perda de fundos | Criptografia AES-256-GCM, senha do usuário |
| Ataque à API pública | DDoS, custos | Rate limiting, API keys, Cloudflare |
| Transação maliciosa | Roubo durante construção | Verificação local antes de broadcast |
| Forjamento de ring members | Quebra de privacidade | Verificação on-chain dos membros |

### 5.2 Boas Práticas

- **Chaves privadas sempre no dispositivo.** Nunca são enviadas para servidor.
- **Seed phrase é o único backup.** Usuário escreve em papel.
- **Criptografia em repouso.** Wallet file criptografado com chave derivada de senha.
- **HTTPS obrigatório.** Todas as comunicações com API pública via TLS.
- **Rate limiting.** Máximo de X requisições por minuto por IP na API pública.
- **Verificação local.** App mobile verifica MLSAG e range proofs antes de enviar tx.

---

## 6. Dependências Externas

### Desktop Miner (adicionais ao core existente)

| Crate | Função |
|-------|--------|
| `egui` | System tray e UI mínima |
| `tray-icon` | Ícone na bandeja do Windows |
| `windows` (crate) | Win32 API: GetLastInputInfo, auto-start registro |
| `self_update` | Auto-update via GitHub Releases |
| `dirs` | Pastas padrão do sistema (AppData) |

### Mobile Wallet

| Pacote | Função |
|--------|--------|
| `react-native` | Framework cross-platform |
| `react-native-camera` | Leitor de QR Code |
| `react-native-qrcode-svg` | Gerador de QR Code |
| `react-native-nfc-manager` | NFC (Android HCE + iOS) |
| `@react-native-async-storage` | Armazenamento local |
| `react-native-keychain` | iOS Keychain access |
| `react-native-get-random-values` | RNG criptográfico |
| `uniffi-rs` | Geração de bindings Rust → Kotlin/Swift |

### Bridge Rust

| Crate | Função |
|-------|--------|
| `uniffi` | Geração de bindings mobile |
| `serde_json` | Serialização de transações |
| `base64` | Encoding para QR Code |
| `wasm-pack` | (Futuro) Build para WebAssembly |

---

## 7. Build Pipeline

```
                    ┌─────────────────────┐
                    │  GitHub Repository  │
                    │  4Ewatts/ewatts-    │
                    │  protocol           │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   GitHub Actions    │
                    └──────────┬──────────┘
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
  ┌───────▼───────┐    ┌───────▼───────┐    ┌───────▼───────┐
  │ Windows .exe  │    │  Android APK  │    │   iOS .ipa    │
  │ (cargo build) │    │  (Kotlin +    │    │  (Swift +     │
  │               │    │   Rust ARM)   │    │   Rust ARM)   │
  └───────────────┘    └───────────────┘    └───────────────┘
         │                     │                     │
         ▼                     ▼                     ▼
  GitHub Releases          Google Play           App Store
```

---

## 8. Próximos Passos

1. Revisar arquitetura
2. Implementar **API pública** (endpoints REST que mobile precisa)
3. Implementar **Desktop Miner MVP** (system tray + idle mining)
4. Implementar **uniffi bridge** (Rust → mobile bindings)
5. Implementar **Mobile Wallet MVP** (React Native)

Arquivo salvo em `App/ARCHITECTURE.md`.
