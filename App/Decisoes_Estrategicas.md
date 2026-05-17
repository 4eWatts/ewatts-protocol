# eWatts — Decisões Estratégicas de Arquitetura

## Contexto

Duas decisões fundamentais precisam ser fechadas antes de implementar qualquer app. 
Elas são independentes entre si mas interdependentes no resultado final.

---

## Decisão 1: Modelo de Adoção

### Opção A: Cypherpunk-first

**Características:**
- Apenas CLI e lib (Rust crate)
- GUI/Mobile são projetos de terceiros, não oficiais
- Early adopters = quem compila Rust, entende MBPoW, lê whitepaper
- Documentação técnica, não tutoriais

**Prós:**
- Zero contradição com a tese ("se você precisa de tutorial, talvez não devesse usar")
- Superfície de ataque mínima (um binário, sem auto-start, sem app stores)
- Atrai exatamente quem o protocolo precisa: operadores técnicos
- Sem dependência de Apple/Google/GitHub para distribuição
- Sem risco de censura via app store

**Contras:**
- Adoção lenta (~0.1% do potencial de usuários casuais)
- Mining limitado a quem sabe compilar Rust e configurar
- Dificulta o "Airbnb da DRAM" (idle mining em PC de família)
- Mobile wallet inexistente no curto prazo

**Veredito:**
> Consistente com a tese. Lento. Seguro. Honesto.

### Opção B: Adoption-friendly com honestidade

**Características:**
- Desktop miner com tray icon (idle mining)
- Mobile wallet via React Native + uniffi
- Mas com infraestrutura explicitamente declarada como bootstrap
- Node público opera sob domínio ewatts.org com compromisso público de:

> "Este node é provido pelo founder durante a fase de bootstrap (12-24 meses). 
> Após este período, a operação será transferida para um pool de operadores independentes 
> ou substituída por conexão P2P direta. O roadmap de descentralização está em 
> https://ewatts.org/decentralization-roadmap"

**Prós:**
- Adoção rápida possível (qualquer um baixa, instala, esquece)
- Mobile wallet viável no curto prazo
- Idle mining cresce hashrate mesmo que marginal
- A honestidade sobre o período de bootstrap preserva credibilidade

**Contras:**
- Dependência de AWS/DigitalOcean durante bootstrap
- Node público como ponto centralizado (mesmo que temporário)
- Risco de censura via cloud provider
- App stores como vetores de censura
- O "temporário" tende a virar permanente

### Conclusão do fundador sobre Decisão 1:

**[A PREENCHER]**

---

## Decisão 2: Como Mobile Wallet Conecta à Rede

### Opção A: Node público centralizado

**Como funciona:**
- Um único node eWatts hospedado pelo founder em servidor cloud
- Mobile wallet consulta balance via GET /api/balance/:addr
- Mobile wallet submete tx via POST /api/submit_tx
- GET /api/ring/pool fornece UTXOs para ring members

**Riscos:**
- Censura: operador do node pode negar serviço a endereços específicos
- Privacidade: operador sabe quais endereços consultam saldo
- Ring pool manipulado: operador fornece UTXOs comprometidos para degradar privacidade
- Single point of failure: se o node cai, toda wallet mobile para de funcionar

### Opção B: Pool de nodes independentes

**Como funciona:**
- Múltiplos operadores rodam nodes públicos (você + early miners + voluntários)
- Mobile wallet randomiza qual node consulta a cada requisição
- Mobile wallet cross-valida respostas entre nodes diferentes
- Fundação (ou comitê informal) mantém lista pública de nodes confiáveis

**Riscos:**
- Requer convencer outros operadores a rodar nodes públicos (~5 mínimo)
- Manutenção da lista de nodes é overhead operacional
- Nodes maliciosos na lista podem fornecer dados incorretos

### Opção C: P2P direto no mobile

**Como funciona:**
- libp2p roda diretamente no dispositivo mobile
- Wallet conecta-se à rede P2P como um peer qualquer
- Sem node intermediário
- Scan de UTXOs local (via filtros BIP157-style ou download de blocos)

**Riscos:**
- libp2p em iOS tem restrições de background networking
- Bateria: P2P ativo drena
- Dados: scan de UTXOs requer dados de blockchain (centenas de MB)
- Complexidade técnica alta

### Conclusão do fundador sobre Decisão 2:

**[A PREENCHER]**

---

## Matriz de Combinacão

| Decisão 1 | Decisão 2 | Resultado |
|-----------|-----------|-----------|
| Cypherpunk | P2P direto | Máximo alinhamento com tese. Lento. Técnico. |
| Cypherpunk | Pool nodes | Consistente. Adoção moderada. Requer rede. |
| Adoption-friendly | Node centralizado | Rápido. Contradiz tese. Risco de credibilidade. |
| Adoption-friendly | Pool nodes | Bom balanço. Requer coordenação. |
| Adoption-friendly | P2P direto | Complexo. Mobile pesado. Tech debt alto. |

---

## Ações Imediatas

Independente das decisões:

1. Remover referências a "no admin keys" e "descentralizado" do site se a opção for node centralizado, ou
2. Adicionar "bootstrap phase" disclaimer explícito no site e no README sobre a infraestrutura centralizada temporária
3. Fechar as duas decisões antes de começar implementação de apps

Documento salvo em `App/Decisoes_Estrategicas.md`.
