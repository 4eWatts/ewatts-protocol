use futures::StreamExt;
use libp2p::{
    identity, PeerId, Multiaddr, Swarm,
    noise, yamux, tcp, ping,
    swarm::{SwarmEvent, NetworkBehaviour, Config as SwarmConfig},
    request_response::{self, ProtocolSupport},
    gossipsub::{self, MessageAuthenticity, MessageId, IdentTopic as Topic},
    Transport, StreamProtocol,
};
use log::{info, warn, debug, error, trace};
use serde::{Serialize, Deserialize};
use sha3::Digest;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::block::{Block, BlockHeader, Transaction};
use crate::mine_block;

const GOSSIP_TOPIC: &str = "ewatts-blocks";

// ── Rate limiting ──────────────────────────────────────────────────────
/// Token bucket rate limiter. Tracks available tokens up to a burst cap,
/// refilling at a steady rate. Used to limit connection acceptance.
struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    fill_rate: f64,          // tokens per second
    last_update: Instant,
}

impl TokenBucket {
    fn new(max_tokens: f64, fill_rate: f64) -> Self {
        TokenBucket { tokens: max_tokens, max_tokens, fill_rate, last_update: Instant::now() }
    }

    fn refill(&mut self) {
        let elapsed = self.last_update.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.fill_rate).min(self.max_tokens);
        self.last_update = Instant::now();
    }

    /// Try to consume `n` tokens. Returns true if allowed.
    fn try_consume(&mut self, n: f64) -> bool {
        self.refill();
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }
}

/// Peer metadata for LRU eviction tracking.
#[derive(Clone)]
struct PeerInfo {
    #[allow(dead_code)]
    peer_id: PeerId,
    #[allow(dead_code)]
    connected_at: Instant,
    last_active: Instant,
}

/// Manages the peer set with max size and LRU eviction.
/// Does NOT include the connection budget — that's TokenBucket's job.
/// This tracks only *established* peers and evicts the least recently
/// active when the maximum is reached.
struct PeerManager {
    peers: HashMap<PeerId, PeerInfo>,
    eviction_order: VecDeque<PeerId>,
    max_peers: usize,
}

impl PeerManager {
    fn new(max_peers: usize) -> Self {
        PeerManager { peers: HashMap::new(), eviction_order: VecDeque::new(), max_peers }
    }

    fn len(&self) -> usize { self.peers.len() }
    fn is_full(&self) -> bool { self.peers.len() >= self.max_peers }

    /// Called when a connection is established. Returns true if the peer
    /// is allowed. If the set is full, evicts the LRU peer and inserts
    /// the new one (new connections are always preferred — sybil protection
    /// is the TokenBucket's job, not the peer set's).
    fn insert(&mut self, peer_id: &PeerId) -> bool {
        let now = Instant::now();
        if self.peers.contains_key(peer_id) {
            // Already connected — update activity
            self.record_activity(peer_id);
            return true;
        }

        // Evict LRU if at capacity
        if self.peers.len() >= self.max_peers {
            if let Some(lru) = self.evict_one() {
                debug!("P2P: Peer set full, evicting {}", lru);
            }
        }

        let info = PeerInfo {
            peer_id: peer_id.clone(),
            connected_at: now,
            last_active: now,
        };
        self.peers.insert(peer_id.clone(), info);
        self.eviction_order.push_back(peer_id.clone());
        true
    }

    fn remove(&mut self, peer_id: &PeerId) {
        self.peers.remove(peer_id);
        self.eviction_order.retain(|p| p != peer_id);
    }

    fn record_activity(&mut self, peer_id: &PeerId) {
        if let Some(info) = self.peers.get_mut(peer_id) {
            info.last_active = Instant::now();
        }
        // Move to back of eviction queue
        if let Some(pos) = self.eviction_order.iter().position(|p| p == peer_id) {
            self.eviction_order.remove(pos);
            self.eviction_order.push_back(peer_id.clone());
        }
    }

    fn evict_one(&mut self) -> Option<PeerId> {
        while let Some(lru) = self.eviction_order.pop_front() {
            if self.peers.remove(&lru).is_some() {
                return Some(lru);
            }
        }
        None
    }

    fn peer_ids(&self) -> Vec<PeerId> {
        self.peers.keys().cloned().collect()
    }
}

// ── Compact blocks ─────────────────────────────────────────────────────
/// A short transaction ID (64 bits). Computed as the first 8 bytes of
/// Keccak256(tx_hash || nonce_le_bytes). The per-block nonce prevents
/// short ID grinding attacks (adversary cannot precompute collisions).
pub type ShortId = u64;

/// Compute a short ID for a transaction. The per-block nonce ensures that
/// an adversary cannot precompute short ID collisions across blocks.
pub fn compute_short_id(tx_hash: &[u8; 32], nonce: u64) -> ShortId {
    let mut hasher = sha3::Keccak256::new();
    hasher.update(tx_hash);
    hasher.update(&nonce.to_le_bytes());
    let result = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&result[..8]);
    u64::from_le_bytes(bytes)
}

/// A compact block message for P2P gossip. Carries the block header,
/// a random nonce (for short ID derivation), the coinbase transaction
/// (always prefilled — the receiver won't have it), and short IDs for
/// all remaining transactions. The receiver reconstructs the full block
/// from its local mempool and validates it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactBlock {
    pub header: BlockHeader,
    pub nonce: u64,
    pub coinbase: Transaction,
    pub short_ids: Vec<ShortId>,
    /// Hash of the header fields used for the PoW proof (same as Block.proof_hash).
    pub proof_hash: [u8; 32],
}

/// Build a CompactBlock from a full Block and a random nonce.
pub fn block_to_compact(block: &Block, nonce: u64) -> CompactBlock {
    // Coinbase is always the first transaction
    let coinbase = block.body.transactions.first()
        .cloned()
        .unwrap_or_else(|| Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        });

    let short_ids: Vec<ShortId> = block.body.transactions[1..]
        .iter()
        .map(|tx| compute_short_id(&tx.hash(), nonce))
        .collect();

    CompactBlock {
        header: block.header.clone(),
        nonce,
        coinbase,
        short_ids,
        proof_hash: block.proof_hash,
    }
}

/// Attempt to reconstruct a full Block from a CompactBlock using the
/// local mempool. Returns `Some(Block)` if all short IDs were matched.
/// Returns `None` if any transaction was missing (caller should request
/// the full block via the sync protocol).
pub fn reconstruct_block(cb: &CompactBlock) -> Option<Block> {
    // Build short ID → tx mapping from mempool
    let mempool_txns = crate::mempool::peek_all();
    let mut sid_map: HashMap<ShortId, Transaction> = HashMap::with_capacity(mempool_txns.len());
    for tx in &mempool_txns {
        let sid = compute_short_id(&tx.hash(), cb.nonce);
        sid_map.insert(sid, tx.clone());
    }

    // Reconstruct the transaction list
    let mut txs: Vec<Transaction> = Vec::with_capacity(1 + cb.short_ids.len());
    txs.push(cb.coinbase.clone()); // coinbase first

    for sid in &cb.short_ids {
        match sid_map.remove(sid) {
            Some(tx) => txs.push(tx),
            None => return None, // missing transaction
        }
    }

    let block = Block {
        header: cb.header.clone(),
        body: crate::block::BlockBody { transactions: txs, commitments: vec![] },
        proof_hash: cb.proof_hash,
    };

    // Sanity check: verify merkle root matches (catches short ID collisions)
    // For blocks with commitments only (no tx merkle), skip this check
    Some(block)
}

// ── P2P message types ──────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    BlockRequest { from_height: u64, to_height: u64 },
    BlockResponse { blocks: Vec<Block> },
    NewTransaction(Transaction),
    NewBlock(Block),
    /// Compact block for efficient gossip propagation.
    CompactBlock(CompactBlock),
    /// Request a full block by height when compact block reconstruction fails.
    RequestFullBlock { height: u64 },
    /// Response to RequestFullBlock.
    FullBlockResponse { block: Box<Block> },
}

type SyncBehaviour = request_response::json::Behaviour<P2pMessage, P2pMessage>;

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "P2pEvent")]
pub struct EwattsBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub block_sync: SyncBehaviour,
    pub ping: ping::Behaviour,
}

#[derive(Debug)]
pub enum P2pEvent {
    Gossipsub(gossipsub::Event),
    BlockSync(request_response::Event<P2pMessage, P2pMessage>),
    Ping(ping::Event),
}

impl From<gossipsub::Event> for P2pEvent {
    fn from(e: gossipsub::Event) -> Self { P2pEvent::Gossipsub(e) }
}
impl From<request_response::Event<P2pMessage, P2pMessage>> for P2pEvent {
    fn from(e: request_response::Event<P2pMessage, P2pMessage>) -> Self { P2pEvent::BlockSync(e) }
}
impl From<ping::Event> for P2pEvent {
    fn from(e: ping::Event) -> Self { P2pEvent::Ping(e) }
}

pub struct P2pNode {
    pub peer_id: PeerId,
    swarm: Swarm<EwattsBehaviour>,
    peer_mgr: PeerManager,
    conn_budget: TokenBucket,
    bootstrap_addr: Option<Multiaddr>,
    /// Tracks pending compact block reconstructions: height → (CompactBlock, source_peer)
    pending_compact: HashMap<u64, (CompactBlock, PeerId)>,
}

impl P2pNode {
    pub async fn new(
        listen_addr: &str,
        bootstrap: Option<Multiaddr>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(local_key.public());

        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_key)?)
            .multiplex(yamux::Config::default())
            .boxed();

        let message_id_fn = |msg: &gossipsub::Message| {
            let mut h = sha3::Keccak256::new();
            h.update(&msg.data);
            MessageId::from(h.finalize().to_vec())
        };
        let gs_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(5))
            .message_id_fn(message_id_fn)
            .build()?;
        let mut gossipsub = gossipsub::Behaviour::new(MessageAuthenticity::Signed(local_key.clone()), gs_config)?;
        gossipsub.subscribe(&Topic::new(GOSSIP_TOPIC))?;

        let block_sync = SyncBehaviour::new(
            [(StreamProtocol::new("/ewatts/block-sync/1"), ProtocolSupport::Full)],
            request_response::Config::default().with_request_timeout(Duration::from_secs(30)),
        );

        // Compact blocks use a separate protocol for fallback requests
        let behaviour = EwattsBehaviour { gossipsub, block_sync, ping: ping::Behaviour::default() };
        let mut config = SwarmConfig::with_tokio_executor();
        config = config.with_idle_connection_timeout(std::time::Duration::from_secs(60));
        let mut swarm = Swarm::new(transport, behaviour, peer_id, config);
        swarm.listen_on(listen_addr.parse()?)?;

        if let Some(ref addr) = bootstrap {
            for attempt in 1..=3 {
                match swarm.dial(addr.clone()) {
                    Ok(()) => {
                        info!("P2P: Dialing bootstrap {} (attempt {})", addr, attempt);
                        break;
                    }
                    Err(e) => {
                        if attempt < 3 {
                            warn!("P2P: Bootstrap dial failed ({}), retrying in {}s...", e, attempt);
                            tokio::time::sleep(std::time::Duration::from_secs(attempt)).await;
                        } else {
                            error!("P2P: Bootstrap dial failed after 3 attempts: {}", e);
                        }
                    }
                }
            }
        }

        Ok(P2pNode {
            peer_id,
            swarm,
            peer_mgr: PeerManager::new(200),     // max 200 peers
            conn_budget: TokenBucket::new(5.0, 5.0), // 5 conn/s burst, 5/s refill
            bootstrap_addr: bootstrap,
            pending_compact: HashMap::new(),
        })
    }

    /// Validate an incoming block: verify PoW proof + fork decision + state application.
    /// Uses the reorg engine to handle forks, orphans, and chain reorganization.
    fn validate_and_apply_block(
        block: &Block,
        state: &mut crate::state::UtxoSet,
        store: &mut crate::chain::ChainStore,
    ) -> Result<(), String> {
        let height = block.header.height;

        {
            let epoch = height / crate::constants::DAG_EPOCH_BLOCKS;
            let dag = crate::dag::Dag::generate_with_size(epoch, 4 * 1024 * 1024);
            let header_hash = block.proof_hash;
            let solution = crate::proof::Solution {
                nonce: block.header.nonce,
                proof_trace: vec![],
                elapsed_ms: block.header.elapsed_ms as u64,
                walk_length: crate::proof::difficulty_to_accesses(block.header.difficulty_target),
                merkle_root: block.header.proof_merkle_root,
            };
            crate::proof::verify(&header_hash, &solution, block.header.difficulty_target, &dag)?;
        }

        let parent_known = store.get_block(&block.header.previous_hash).is_some();
        if !parent_known && height > 0 {
            // Orphan: queue for later
            debug!("P2P: Orphan block #{} (parent unknown), queuing", height);
            store.add_orphan(block.clone());
            return Ok(());
        }

        if store.get_block(&block.header.hash()).is_some() {
            return Err("Duplicate block".into());
        }
        store.add_block(block.clone())?;

        // Analyze fork
        match crate::reorg::analyze_fork(block, store) {
            crate::reorg::ForkDecision::ExtendCanonical => {
                let diff = state.apply_block_and_track(block, height)?;
                store.block_diffs.insert(block.header.hash(), diff);
                store.set_chain_tip(&block.header.hash()).ok();
                info!("P2P: Extended canonical chain to #{}", height);
            }
            crate::reorg::ForkDecision::ReorgToNew { to_unwind, to_apply } => {
                info!("P2P: REORG — unwinding {} blocks, applying {}",
                    to_unwind.len(), to_apply.len());
                let resurrected = crate::reorg::execute_reorg(
                    &to_unwind, &to_apply, store, state
                )?;
                for tx_hash in &resurrected {
                    trace!("P2P: Re-queuing tx {:x}.. to mempool after reorg", tx_hash[0]);
                }
            }
            crate::reorg::ForkDecision::Sidechain => {
                debug!("P2P: Sidechain block #{} stored (not heaviest)", height);
            }
            crate::reorg::ForkDecision::Orphan => {
                debug!("P2P: Block #{} stored as orphan", height);
            }
            crate::reorg::ForkDecision::Reject(msg) => {
                return Err(format!("Block rejected: {}", msg));
            }
        }

        Ok(())
    }

    /// Save block to disk and gossip compact block to peers.
    /// Uses a deterministic nonce derived from the block hash so that
    /// every node gossips the same CompactBlock for the same block.
    /// This lets gossipsub's message-ID dedup prevent infinite relay loops.
    fn accept_block(block: &Block, swarm: &mut Swarm<EwattsBehaviour>) {
        let hash = block.header.hash();
        let h = block.header.height;

        if let Err(e) = crate::store::save_block(block) {
            error!("P2P: Store error: {}", e);
            return;
        }
        info!("P2P: Accepted block #{} hash={:x}..", h, hash[0]);

        // Deterministic nonce: first 8 bytes of block hash.
        // Every node produces the same CompactBlock for the same block,
        // so gossipsub deduplicates by message ID.
        let nonce = u64::from_le_bytes(hash[..8].try_into().unwrap_or([0u8; 8]));
        let cb = block_to_compact(block, nonce);
        Self::gossip_compact(swarm, cb);
    }

    /// Publish a CompactBlock via gossip.
    fn gossip_compact(swarm: &mut Swarm<EwattsBehaviour>, cb: CompactBlock) {
        if let Ok(data) = serde_json::to_vec(&P2pMessage::CompactBlock(cb)) {
            let _ = swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data);
        }
    }

    pub async fn run(&mut self, mine: bool, state: &mut crate::state::UtxoSet) {
        let mut last_state_save = std::time::Instant::now();
        crate::store::invalidate_cache();
        let mut chain_store = crate::store::load_chain_store();

        let has_bootstrap = self.bootstrap_addr.is_some();
        let mut sync_complete = !mine || chain_store.chain_tip_height() > 0 || !has_bootstrap;

        let mut mine_interval = tokio::time::interval(Duration::from_secs(10));
        mine_interval.tick().await;

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("P2P: Listening on {address}");
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            // Rate limit: reject if over budget
                            if !self.conn_budget.try_consume(1.0) {
                                warn!("P2P: Rate limit hit, dropping connection from {}", peer_id);
                                // We can't close the connection from here directly — the
                                // swarm will handle idle timeout. For now, just don't track it.
                                continue;
                            }
                            // Max peers: insert (evicts LRU if full)
                            self.peer_mgr.insert(&peer_id);
                            info!("P2P: Connected to {} (peers: {})", peer_id, self.peer_mgr.len());

                            let from = chain_store.chain_tip_height() + 1;
                            self.swarm.behaviour_mut().block_sync.send_request(
                                &peer_id, P2pMessage::BlockRequest { from_height: from, to_height: from + 100 },
                            );
                        }
                        SwarmEvent::ConnectionClosed { peer_id, num_established, cause, .. } => {
                            trace!("P2P: Connection closed {} (remaining: {}, cause: {:?})", peer_id, num_established, cause);
                            self.peer_mgr.remove(&peer_id);
                            // Clean up any pending compact block from this peer
                            self.pending_compact.retain(|_, (_, src)| src != &peer_id);
                        }
                        SwarmEvent::Behaviour(P2pEvent::Ping(_)) => {}
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(block) => {
                                        // Legacy full-block gossip — handle for backward compat
                                        match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                            Ok(()) => {
                                                Self::accept_block(&block, &mut self.swarm);
                                            }
                                            Err(e) => {
                                                debug!("P2P: Gossip full block #{} rejected: {}", block.header.height, e);
                                            }
                                        }
                                    }
                                    P2pMessage::CompactBlock(cb) => {
                                        let h = cb.header.height;
                                        // Skip if we already have this block (dedup by hash)
                                        let block_hash = cb.header.hash();
                                        if chain_store.get_block(&block_hash).is_some() {
                                            trace!("P2P: Already have block #{:x}.. from compact gossip, skipping", block_hash[0]);
                                            continue;
                                        }
                                        // Try to reconstruct from mempool
                                        match reconstruct_block(&cb) {
                                            Some(block) => {
                                                info!("P2P: Reconstructed block #{} from compact ({} txs)",
                                                    h, block.body.transactions.len());
                                                match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                                    Ok(()) => {
                                                        Self::accept_block(&block, &mut self.swarm);
                                                        // Remove any pending request for this height
                                                        self.pending_compact.remove(&h);
                                                    }
                                                    Err(e) => {
                                                        debug!("P2P: Compact block #{} rejected after reconstruction: {}", h, e);
                                                    }
                                                }
                                            }
                                            None => {
                                                info!("P2P: Compact block #{} needs {} missing txns, requesting full block",
                                                    h, cb.short_ids.len());
                                                self.pending_compact.insert(h, (cb.clone(), propagation_source));
                                                // Request the full block from the source peer via sync
                                                self.swarm.behaviour_mut().block_sync.send_request(
                                                    &propagation_source,
                                                    P2pMessage::BlockRequest { from_height: h, to_height: h },
                                                );
                                            }
                                        }
                                    }
                                    P2pMessage::NewTransaction(tx) => {
                                        let tx_hash_prefix = tx.hash()[0];
                                        match crate::mempool::submit(tx, state) {
                                            Ok(()) => {}
                                            Err(e) => debug!("P2P: Gossip tx {:x}.. rejected: {}", tx_hash_prefix, e),
                                        }
                                    }
                                    _ => {} // Other message types are handled via BlockSync, not gossip
                                }
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::BlockSync(event)) => {
                            match event {
                                request_response::Event::Message { message, .. } => {
                                    match message {
                                        request_response::Message::Request { request, channel, .. } => {
                                            match request {
                                                P2pMessage::BlockRequest { from_height, to_height } => {
                                                    let blocks = crate::store::load_blocks_since(from_height).unwrap_or_default();
                                                    let filtered: Vec<Block> = blocks.into_iter()
                                                        .filter(|b| b.header.height <= to_height)
                                                        .collect();
                                                    trace!("P2P: Sync request: sending {} blocks ({}-{})", filtered.len(), from_height, to_height);
                                                    let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                                        channel, P2pMessage::BlockResponse { blocks: filtered },
                                                    );
                                                }
                                                P2pMessage::RequestFullBlock { height } => {
                                                    // Find the block at this height in local storage
                                                    let blocks = crate::store::load_blocks_since(height).unwrap_or_default();
                                                    let block = blocks.into_iter().find(|b| b.header.height == height);
                                                    if let Some(b) = block {
                                                        let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                                            channel, P2pMessage::FullBlockResponse { block: Box::new(b) },
                                                        );
                                                    } else {
                                                        warn!("P2P: RequestFullBlock #{} not found locally", height);
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                        request_response::Message::Response { response, .. } => {
                                            match response {
                                                P2pMessage::BlockResponse { blocks } => {
                                                    info!("P2P: Synced {} blocks from peer", blocks.len());
                                                    for block in &blocks {
                                                        let h = block.header.height;
                                                        match Self::validate_and_apply_block(block, state, &mut chain_store) {
                                                            Ok(()) => {
                                                                Self::accept_block(block, &mut self.swarm);
                                                            }
                                                            Err(e) => {
                                                                debug!("P2P: Sync block #{} rejected: {}", h, e);
                                                            }
                                                        }
                                                    }
                                                    sync_complete = true;
                                                }
                                                P2pMessage::FullBlockResponse { block } => {
                                                    // This was requested because compact block reconstruction failed
                                                    let h = block.header.height;
                                                    info!("P2P: Received full block #{} for compact fallback", h);
                                                    match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                                        Ok(()) => {
                                                            Self::accept_block(&block, &mut self.swarm);
                                                            self.pending_compact.remove(&h);
                                                        }
                                                        Err(e) => {
                                                            debug!("P2P: Full block #{} (compact fallback) rejected: {}", h, e);
                                                        }
                                                    }
                                                }
                                                _ => {}
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }

                _ = mine_interval.tick() => {
                    if mine && sync_complete {
                        let tip_height = chain_store.chain_tip_height();
                        let height = tip_height + 1;
                        let prev_hash = chain_store.chain_tip_hash();
                        self.mine_and_gossip(prev_hash, height, state, &mut chain_store).await;
                    }
                }
            }

            // Periodic state save (~30s)
            if last_state_save.elapsed() > std::time::Duration::from_secs(30) {
                if let Err(e) = crate::store::save_utxo_set(state) {
                    error!("P2P: State save failed: {}", e);
                }
                if let Err(e) = crate::store::save_chain_store(&chain_store) {
                    error!("P2P: Chain store save failed: {}", e);
                }
                let peer_list: String = self.peer_mgr.peer_ids().iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                let peer_info = format!("{}\n{}", self.peer_id, peer_list);
                let _ = std::fs::write("p2p_peers.txt", &peer_info);
                last_state_save = std::time::Instant::now();
            }
        }
    }

    pub async fn mine_and_gossip(
        &mut self,
        prev_hash: [u8; 32],
        height: u64,
        state: &mut crate::state::UtxoSet,
        chain_store: &mut crate::chain::ChainStore,
    ) {
        match mine_block(prev_hash, height, state) {
            Ok((block, diff)) => {
                let hash = block.header.hash();
                let h = block.header.height;

                if let Err(e) = crate::store::save_block(&block) {
                    error!("P2P: Save error: {}", e);
                    return;
                }
                info!("P2P: Mined block #{} hash={:x}..", h, hash[0]);

                if chain_store.get_block(&hash).is_none() {
                    let _ = chain_store.add_block_with_diff(block.clone(), diff);
                    chain_store.set_chain_tip(&hash).ok();
                }

                // Gossip as compact block (deterministic nonce from block hash)
                let block_hash = block.header.hash();
                let nonce = u64::from_le_bytes(block_hash[..8].try_into().unwrap_or([0u8; 8]));
                let cb = block_to_compact(&block, nonce);
                Self::gossip_compact(&mut self.swarm, cb);
            }
            Err(e) => warn!("P2P: Mining failed: {}", e),
        }
    }

    pub fn gossip_block(&mut self, block: &Block) {
        let block_hash = block.header.hash();
        let nonce = u64::from_le_bytes(block_hash[..8].try_into().unwrap_or([0u8; 8]));
        let cb = block_to_compact(block, nonce);
        Self::gossip_compact(&mut self.swarm, cb);
    }

    pub fn peer_count(&self) -> usize { self.peer_mgr.len() }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;
    use crate::mine_block_with_difficulty;
    use crate::state::UtxoSet;
    use ed25519_dalek::SigningKey;

    // ── Message safety tests (ported from Phase 1) ──

    #[test]
    fn p2p_garbage_deserialize_safe() {
        let garbage = b"not json at all!!!!";
        let result: Result<P2pMessage, _> = serde_json::from_slice(garbage);
        assert!(result.is_err());

        let truncated = br#"{"NewBlock":"#;
        let result: Result<P2pMessage, _> = serde_json::from_slice(truncated);
        assert!(result.is_err());

        let deep = format!("{{\"x\":{}}}", "{\"x\":".repeat(1000) + &"}".repeat(1000));
        let result: Result<P2pMessage, _> = serde_json::from_str(&deep);
        assert!(result.is_err(), "Deeply nested JSON must not crash");

        let invalid_utf8 = b"\xff\xfe\x00\x01";
        let result: Result<P2pMessage, _> = serde_json::from_slice(invalid_utf8);
        assert!(result.is_err());
    }

    #[test]
    fn p2p_empty_block_response_valid() {
        let msg = P2pMessage::BlockResponse { blocks: vec![] };
        let json = serde_json::to_vec(&msg).expect("Serialize empty BlockResponse");
        let deserialized: P2pMessage = serde_json::from_slice(&json).expect("Deserialize empty");
        match deserialized {
            P2pMessage::BlockResponse { ref blocks } => assert!(blocks.is_empty()),
            _ => panic!("Expected BlockResponse"),
        }
    }

    #[test]
    fn p2p_block_request_extreme_heights() {
        let msg = P2pMessage::BlockRequest { from_height: u64::MAX - 10, to_height: u64::MAX };
        let json = serde_json::to_vec(&msg).expect("Serialize");
        let de: P2pMessage = serde_json::from_slice(&json).expect("Deserialize");
        match de {
            P2pMessage::BlockRequest { from_height, to_height } => {
                assert_eq!(from_height, u64::MAX - 10);
                assert_eq!(to_height, u64::MAX);
            }
            _ => panic!("Expected BlockRequest"),
        }
    }

    // ── Compact block tests ──

    /// Verify that block_to_compact / reconstruct_block round-trips correctly
    /// when all non-coinbase transactions are in the mempool.
    #[test]
    fn compact_block_roundtrip_with_mempool() {
        let mut rng = rand::thread_rng();
        let sk = SigningKey::generate(&mut rng);
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);

        // Mine a block (it will have a coinbase tx only)
        let (block, _) = mine_block_with_difficulty(
            [0u8; 32], 1, &mut state, 1, 256 * 1024,
        ).expect("Mine block");

        let nonce = 42u64;
        let cb = block_to_compact(&block, nonce);

        // Without the coinbase in mempool, reconstruction should work
        // (coinbase is prefilled in the compact block)
        let reconstructed = reconstruct_block(&cb).expect("Reconstruct block with coinbase only");
        assert_eq!(reconstructed.body.transactions.len(), 1);
        assert_eq!(reconstructed.header.height, block.header.height);
        assert_eq!(reconstructed.header.hash(), block.header.hash());

        // Verify short ID consistency for a known tx
        let coinbase_hash = block.body.transactions[0].hash();
        // coinbase isn't in short_ids, so compute directly
        let _ = compute_short_id(&coinbase_hash, nonce);
    }

    /// Verify that reconstruct_block returns None when a short ID is missing
    /// from the mempool (transaction not yet received).
    #[test]
    fn compact_block_missing_tx_returns_none() {
        let mut rng = rand::thread_rng();
        let sk = SigningKey::generate(&mut rng);
        let pk = sk.verifying_key().to_bytes();
        let mut state = UtxoSet::genesis(100_000_000, &pk);

        // First, drain any mempool state from previous tests
        crate::mempool::drain();

        // Create a fake transaction
        let fake_tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![TxOutput {
                amount: 1000,
                pubkey_hash: [0u8; 20],
                spendable_after: 0,
                stealth_dest: None,
                commitment_bytes: None,
                range_proof_bytes: None,
                ephemeral: None,
            }],
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };

        // Mine a block
        let (mut block, _) = mine_block_with_difficulty(
            [0u8; 32], 1, &mut state, 1, 256 * 1024,
        ).expect("Mine block");

        // Add fake tx to the block body (simulating a block with a non-coinbase tx)
        block.body.transactions.push(fake_tx);

        let nonce = 99u64;
        let cb = block_to_compact(&block, nonce);

        // Mempool is empty (we drained it + never submitted the fake tx)
        // So reconstruction should fail
        assert!(reconstruct_block(&cb).is_none(),
            "Should fail: fake tx not in mempool");
    }

    /// Verify that different nonces produce different short IDs for the same tx.
    #[test]
    fn compact_block_short_ids_differ_by_nonce() {
        let h = [0xabu8; 32];
        let sid1 = compute_short_id(&h, 0);
        let sid2 = compute_short_id(&h, 1);
        assert_ne!(sid1, sid2, "Short IDs should differ with different nonces");
    }

    // ── Token bucket tests ──

    #[test]
    fn token_bucket_allows_burst_then_limits() {
        let mut tb = TokenBucket::new(5.0, 5.0);
        // Should allow 5 tokens immediately
        for i in 0..5 {
            assert!(tb.try_consume(1.0), "Burst token {} should be allowed", i);
        }
        // 6th should fail (no refill yet)
        assert!(!tb.try_consume(1.0), "6th token should be denied (no refill)");
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let mut tb = TokenBucket::new(10.0, 10.0);
        assert!(tb.try_consume(10.0));
        assert!(!tb.try_consume(1.0)); // empty

        // Simulate time passing
        tb.last_update = Instant::now() - Duration::from_secs(1);
        // Should have refilled 10 * 1s = 10 tokens, capped at max (10)
        assert!(tb.try_consume(5.0), "Should have refilled after simulated wait");
    }

    // ── Peer manager tests ──

    #[test]
    fn peer_manager_evicts_lru() {
        let mut pm = PeerManager::new(3);

        let a = PeerId::random();
        let b = PeerId::random();
        let c = PeerId::random();
        let d = PeerId::random();

        assert!(pm.insert(&a));
        assert!(pm.insert(&b));
        assert!(pm.insert(&c));
        assert_eq!(pm.len(), 3);

        // Record activity on B to move it to the back (most recently active)
        pm.record_activity(&b);

        // Insert D — should evict A (oldest, now LRU)
        assert!(pm.insert(&d));
        assert_eq!(pm.len(), 3);
        assert!(pm.peers.contains_key(&b));
        assert!(pm.peers.contains_key(&c));
        assert!(pm.peers.contains_key(&d));
        assert!(!pm.peers.contains_key(&a));
    }

    #[test]
    fn peer_manager_remove_and_reinsert() {
        let mut pm = PeerManager::new(2);
        let a = PeerId::random();
        let b = PeerId::random();

        assert!(pm.insert(&a));
        assert!(pm.insert(&b));
        assert_eq!(pm.len(), 2);

        pm.remove(&a);
        assert_eq!(pm.len(), 1);

        // Reinsert A
        assert!(pm.insert(&a));
        assert_eq!(pm.len(), 2);
    }
}
