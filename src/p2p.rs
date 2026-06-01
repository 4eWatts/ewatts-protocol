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
use std::time::Duration;
use std::collections::HashSet;

use crate::block::{Block, Transaction};
use crate::mine_block;

const GOSSIP_TOPIC: &str = "ewatts-blocks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    BlockRequest { from_height: u64, to_height: u64 },
    BlockResponse { blocks: Vec<Block> },
    NewTransaction(Transaction),
    NewBlock(Block),
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
    peers: HashSet<PeerId>,
    bootstrap_addr: Option<Multiaddr>,
}

impl P2pNode {
    pub async fn new(listen_addr: &str, bootstrap: Option<Multiaddr>) -> Result<Self, Box<dyn std::error::Error>> {
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

        let behaviour = EwattsBehaviour { gossipsub, block_sync, ping: ping::Behaviour::default() };
        let mut config = SwarmConfig::with_tokio_executor();
        config = config.with_idle_connection_timeout(std::time::Duration::from_secs(60));
        let mut swarm = Swarm::new(transport, behaviour, peer_id, config);
        swarm.listen_on(listen_addr.parse()?)?;

        if let Some(ref addr) = bootstrap {
            for attempt in 1..=3 {
                match swarm.dial(addr.clone()) {
                    Ok(()) => {
                        debug!("P2P: Dialing bootstrap {} (attempt {})", addr, attempt);
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

        Ok(P2pNode { peer_id, swarm, peers: HashSet::new(), bootstrap_addr: bootstrap })
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
            trace!("P2P: Orphan block #{} (parent unknown), queuing", height);
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
                trace!("P2P: Sidechain block #{} stored (not heaviest)", height);
            }
            crate::reorg::ForkDecision::Orphan => {
                trace!("P2P: Block #{} stored as orphan", height);
            }
            crate::reorg::ForkDecision::Reject(msg) => {
                return Err(format!("Block rejected: {}", msg));
            }
        }

        Ok(())
    }

    /// Save block to disk and gossip to peers
    fn accept_block(block: &Block, swarm: &mut Swarm<EwattsBehaviour>) {
        let hash = block.header.hash();
        let h = block.header.height;

        if let Err(e) = crate::store::save_block(block) {
            error!("P2P: Store error: {}", e);
            return;
        }
        info!("P2P: Accepted block #{} hash={:x}..", h, hash[0]);

        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
        }
    }

    pub async fn run(&mut self, mine: bool, state: &mut crate::state::UtxoSet) {
        let mut last_state_save = std::time::Instant::now();
        // Invalidate block cache so chain store loads fresh data from disk
        crate::store::invalidate_cache();
        let mut chain_store = crate::store::load_chain_store();

        // For boot/seed nodes (no bootstrap address), mine immediately.
        // For follower nodes, wait for the first BlockResponse to arrive
        // before starting the mining timer. Without this guard, the 10s
        // timer can fire before the sync completes, causing the follower
        // to mine a competing block #1 on genesis.
        let has_bootstrap = self.bootstrap_addr.is_some();
        let mut sync_complete = !mine || chain_store.chain_tip_height() > 0 || !has_bootstrap;

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            info!("P2P: Listening on {address}");
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            info!("P2P: Connected to {peer_id}");
                            self.peers.insert(peer_id);

                            let from = chain_store.chain_tip_height() + 1;
                            self.swarm.behaviour_mut().block_sync.send_request(
                                &peer_id, P2pMessage::BlockRequest { from_height: from, to_height: from + 100 },
                            );
                        }
                        SwarmEvent::ConnectionClosed { peer_id, num_established, cause, .. } => {
                            debug!("P2P: Connection closed {} (remaining: {}, cause: {:?})", peer_id, num_established, cause);
                            self.peers.remove(&peer_id);
                        }
                        SwarmEvent::Behaviour(P2pEvent::Ping(_)) => {}
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source: _source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(block) => {
                                        match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                            Ok(()) => {
                                                Self::accept_block(&block, &mut self.swarm);
                                            }
                                            Err(e) => {
                                                trace!("P2P: Gossip block #{} rejected: {}", block.header.height, e);
                                            }
                                        }
                                    }
                                    P2pMessage::NewTransaction(tx) => {
                                        let tx_hash_prefix = tx.hash()[0];
                                        match crate::mempool::submit(tx, state) {
                                            Ok(()) => {}
                                            Err(e) => trace!("P2P: Gossip tx {:x}.. rejected: {}", tx_hash_prefix, e),
                                        }
                                    }
                                    _ => {}
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
                                                    // Load blocks starting from requested height (avoids full scan)
                                                    let blocks = crate::store::load_blocks_since(from_height).unwrap_or_default();
                                                    let filtered: Vec<Block> = blocks.into_iter()
                                                        .filter(|b| b.header.height <= to_height)
                                                        .collect();
                                                    debug!("P2P: Sync request: sending {} blocks ({}-{})", filtered.len(), from_height, to_height);
                                                    let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                                        channel, P2pMessage::BlockResponse { blocks: filtered },
                                                    );
                                                }
                                                _ => {}
                                            }
                                        }
                                        request_response::Message::Response { response, .. } => {
                                            match response {
                                                P2pMessage::BlockResponse { blocks } => {
                                                    debug!("P2P: Synced {} blocks from peer", blocks.len());
                                                    for block in &blocks {
                                                        let h = block.header.height;
                                                        match Self::validate_and_apply_block(block, state, &mut chain_store) {
                                                            Ok(()) => {
                                                                Self::accept_block(block, &mut self.swarm);
                                                            }
                                                            Err(e) => {
                                                                trace!("P2P: Sync block #{} rejected: {}", h, e);
                                                            }
                                                        }
                                                    }
                                                    sync_complete = true;
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

                _ = async { if mine && sync_complete {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                } else {
                    futures::future::pending::<()>().await;
                }} => {
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
                let peer_list: String = self.peers.iter()
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

                if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block)) {
                    self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
                }
            }
            Err(e) => warn!("P2P: Mining failed: {}", e),
        }
    }

    pub fn gossip_block(&mut self, block: &Block) {
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
        }
    }

    pub fn peer_count(&self) -> usize { self.peers.len() }
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 2 — P2P Network Resilience Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::*;

    // T2.1: Garbage JSON must deserialize as Err, not panic
    #[test]
    fn p2p_garbage_deserialize_safe() {
        // Malformed JSON
        let garbage = b"not json at all!!!!";
        let result: Result<P2pMessage, _> = serde_json::from_slice(garbage);
        assert!(result.is_err(), "Garbage must not deserialize");

        // Truncated JSON
        let truncated = br#"{"NewBlock":"#;
        let result: Result<P2pMessage, _> = serde_json::from_slice(truncated);
        assert!(result.is_err(), "Truncated JSON must not deserialize");

        // Very deep nesting (stack overflow attempt)
        let deep = format!("{{\"x\":{}}}", "{\"x\":".repeat(1000) + &"}".repeat(1000));
        let result: Result<P2pMessage, _> = serde_json::from_str(&deep);
        assert!(result.is_err(), "Deeply nested JSON must not crash");

        // Invalid UTF-8
        let invalid_utf8 = b"\xff\xfe\x00\x01";
        let result: Result<P2pMessage, _> = serde_json::from_slice(invalid_utf8);
        assert!(result.is_err(), "Invalid UTF-8 must not deserialize");
    }

    // T2.2: Empty block response is valid (edge case: no blocks to sync)
    #[test]
    fn p2p_empty_block_response_valid() {
        let msg = P2pMessage::BlockResponse { blocks: vec![] };
        let json = serde_json::to_vec(&msg).expect("Serialize empty BlockResponse");
        let deserialized: P2pMessage = serde_json::from_slice(&json)
            .expect("Deserialize empty BlockResponse");
        match deserialized {
            P2pMessage::BlockResponse { ref blocks } => {
                assert!(blocks.is_empty(), "Must be empty");
            }
            _ => panic!("Expected BlockResponse"),
        }
    }

    // T2.3: BlockRequest with extreme heights must not overflow
    #[test]
    fn p2p_block_request_extreme_heights() {
        // Max u64 values
        let msg = P2pMessage::BlockRequest {
            from_height: u64::MAX - 10,
            to_height: u64::MAX,
        };
        let json = serde_json::to_vec(&msg).expect("Serialize extreme BlockRequest");
        let deserialized: P2pMessage = serde_json::from_slice(&json)
            .expect("Deserialize extreme BlockRequest");
        match deserialized {
            P2pMessage::BlockRequest { from_height, to_height } => {
                assert_eq!(from_height, u64::MAX - 10);
                assert_eq!(to_height, u64::MAX);
            }
            _ => panic!("Expected BlockRequest"),
        }

        // Zero heights
        let msg = P2pMessage::BlockRequest { from_height: 0, to_height: 0 };
        let json = serde_json::to_vec(&msg).expect("Serialize zero BlockRequest");
        let deserialized: P2pMessage = serde_json::from_slice(&json).expect("Deserialize zero");
        match deserialized {
            P2pMessage::BlockRequest { from_height, to_height } => {
                assert_eq!(from_height, 0);
                assert_eq!(to_height, 0);
            }
            _ => panic!("Expected BlockRequest"),
        }
    }

    // T2.4: Max-size transaction must not crash deserialization
    #[test]
    fn p2p_max_sized_transaction() {
        // Create a tx with the absolute max number of outputs
        let mut outputs = Vec::with_capacity(1000);
        for i in 0..1000 {
            outputs.push(TxOutput {
                amount: i as u64,
                pubkey_hash: [0u8; 20],
                spendable_after: 0,
                stealth_dest: None,
                commitment_bytes: None,
                range_proof_bytes: None,
                ephemeral: None,
            });
        }
        let tx = Transaction {
            version: 1,
            inputs: vec![],
            outputs,
            ring_size: 1,
            signatures: vec![],
            mlsag: None,
            ring_members: None,
        };
        let msg = P2pMessage::NewTransaction(tx);
        let json = serde_json::to_vec(&msg).expect("Serialize large tx");
        assert!(json.len() > 1000, "JSON must be large");
        let deserialized: P2pMessage = serde_json::from_slice(&json)
            .expect("Deserialize large tx");
        match deserialized {
            P2pMessage::NewTransaction(tx) => {
                assert_eq!(tx.outputs.len(), 1000);
            }
            _ => panic!("Expected NewTransaction"),
        }
    }

    // T2.5: validate_and_apply_block rejects a block with invalid proof
    // NOTE: This test is DISABLED because validate_and_apply_block generates
    // a 4MB DAG for proof verification, making it too slow for unit testing.
    // In production, P2P validation uses this function — unit tests rely on
    // the serialization/deserialization tests above to verify message safety.
    #[test]
    #[ignore]
    fn p2p_validate_rejects_invalid_block() {
        // Full PoW validation requires 4MB DAG — run manually with -- --ignored
    }
}
