use futures::StreamExt;
use libp2p::{
    identity, PeerId, Multiaddr, Swarm,
    noise, yamux, tcp,
    swarm::{SwarmEvent, NetworkBehaviour, Config as SwarmConfig},
    request_response::{self, ProtocolSupport},
    gossipsub::{self, MessageAuthenticity, MessageId, IdentTopic as Topic},
    Transport, StreamProtocol,
};
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
}

#[derive(Debug)]
pub enum P2pEvent {
    Gossipsub(gossipsub::Event),
    BlockSync(request_response::Event<P2pMessage, P2pMessage>),
}

impl From<gossipsub::Event> for P2pEvent {
    fn from(e: gossipsub::Event) -> Self { P2pEvent::Gossipsub(e) }
}
impl From<request_response::Event<P2pMessage, P2pMessage>> for P2pEvent {
    fn from(e: request_response::Event<P2pMessage, P2pMessage>) -> Self { P2pEvent::BlockSync(e) }
}

pub struct P2pNode {
    pub peer_id: PeerId,
    swarm: Swarm<EwattsBehaviour>,
    peers: HashSet<PeerId>,
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

        let behaviour = EwattsBehaviour { gossipsub, block_sync };
        let config = SwarmConfig::with_tokio_executor();
        let mut swarm = Swarm::new(transport, behaviour, peer_id, config);
        swarm.listen_on(listen_addr.parse()?)?;

        if let Some(addr) = bootstrap {
            swarm.dial(addr).ok();
        }

        Ok(P2pNode { peer_id, swarm, peers: HashSet::new() })
    }

    /// Validate an incoming block: verify PoW proof + fork decision + state application.
    /// Uses the reorg engine to handle forks, orphans, and chain reorganization.
    fn validate_and_apply_block(
        block: &Block,
        state: &mut crate::state::UtxoSet,
        store: &mut crate::chain::ChainStore,
    ) -> Result<(), String> {
        let height = block.header.height;

        // 1. Verify the block's proof-of-work
        {
            let epoch = height / crate::constants::DAG_EPOCH_BLOCKS;
            let dag = crate::dag::Dag::generate_with_size(epoch, 4 * 1024 * 1024);
            let header_hash = block.header.hash();
            let solution = crate::proof::Solution {
                nonce: block.header.nonce,
                proof_trace: vec![],
                elapsed_ms: block.header.elapsed_ms as u64,
                walk_length: crate::proof::difficulty_to_accesses(block.header.difficulty_target),
            };
            crate::proof::verify(&header_hash, &solution, block.header.difficulty_target, &dag)?;
        }

        // 2. Check the store for the block's parent
        let parent_known = store.get_block(&block.header.previous_hash).is_some();
        if !parent_known && height > 0 {
            // Orphan block: store for later
            println!("P2P: Orphan block #{} (parent unknown), queuing", height);
            store.add_orphan(block.clone());
            return Ok(());
        }

        // 3. Add block to the chain store (establishes parent-child relationship)
        if store.get_block(&block.header.hash()).is_some() {
            return Err("Duplicate block".into());
        }
        store.add_block(block.clone())?;

        // 4. Analyze fork and decide action
        match crate::reorg::analyze_fork(block, store) {
            crate::reorg::ForkDecision::ExtendCanonical => {
                // Apply to state and capture diff for reorg
                let diff = state.apply_block_and_track(block, height)?;
                store.block_diffs.insert(block.header.hash(), diff);
                store.set_chain_tip(&block.header.hash()).ok();
                println!("P2P: Extended canonical chain to #{}", height);
            }
            crate::reorg::ForkDecision::ReorgToNew { to_unwind, to_apply } => {
                println!("P2P: REORG — unwinding {} blocks, applying {}",
                    to_unwind.len(), to_apply.len());
                let resurrected = crate::reorg::execute_reorg(
                    &to_unwind, &to_apply, store, state
                )?;
                // Return resurrected txs to mempool
                for tx_hash in &resurrected {
                    println!("P2P: Re-queuing tx {:x}.. to mempool after reorg", tx_hash[0]);
                }
            }
            crate::reorg::ForkDecision::Sidechain => {
                println!("P2P: Sidechain block #{} stored (not heaviest)", height);
                // Block is stored in the tree, not applied to state.
                // If the sidechain becomes heavier later, a reorg will be triggered.
            }
            crate::reorg::ForkDecision::Orphan => {
                println!("P2P: Block #{} stored as orphan", height);
            }
            crate::reorg::ForkDecision::Reject(msg) => {
                // Block was already in store — remove the duplicate entry
                // (Actually, add_block() was a no-op for duplicates)
                return Err(format!("Block rejected: {}", msg));
            }
        }

        Ok(())
    }

    /// Receive a validated block: save, update cache, gossip to peers.
    fn accept_block(block: &Block, swarm: &mut Swarm<EwattsBehaviour>) {
        let hash = block.header.hash();
        let h = block.header.height;

        if let Err(e) = crate::store::save_block(block) {
            eprintln!("P2P: Store error: {}", e);
            return;
        }
        println!("P2P: Accepted block #{} hash={:x}..", h, hash[0]);

        // Gossip to peers
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
        }
    }

    pub async fn run(&mut self, mine: bool, state: &mut crate::state::UtxoSet) {
        let mut last_state_save = std::time::Instant::now();
        // Load or initialize fork-aware chain store
        let mut chain_store = crate::store::load_chain_store();

        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("P2P: Listening on {address}");
                        }
                        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                            println!("P2P: Connected to {peer_id}");
                            self.peers.insert(peer_id);

                            // Request latest blocks from new peer
                            let from = chain_store.chain_tip_height() + 1;
                            self.swarm.behaviour_mut().block_sync.send_request(
                                &peer_id, P2pMessage::BlockRequest { from_height: from, to_height: from + 100 },
                            );
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            self.peers.remove(&peer_id);
                        }
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source: _source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(block) => {
                                        let h = block.header.height;
                                        println!("P2P: Gossip block #{} received", h);
                                        match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                            Ok(()) => {
                                                Self::accept_block(&block, &mut self.swarm);
                                                // Save chain store state to disk periodically
                                            }
                                            Err(e) => {
                                                println!("P2P: Gossip block #{} rejected: {}", h, e);
                                            }
                                        }
                                    }
                                    P2pMessage::NewTransaction(tx) => {
                                        // Validate and submit to mempool
                                        let tx_hash = tx.hash();
                                        match crate::mempool::submit(tx, state) {
                                            Ok(()) => println!("P2P: Gossip tx {:x}.. accepted", tx_hash[0]),
                                            Err(e) => println!("P2P: Gossip tx {:x}.. rejected: {}", tx_hash[0], e),
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
                                                    let blocks = crate::store::load_blocks().unwrap_or_default();
                                                    let filtered: Vec<Block> = blocks.into_iter()
                                                        .filter(|b| b.header.height >= from_height && b.header.height <= to_height)
                                                        .collect();
                                                    println!("P2P: Sync request: sending {} blocks ({}-{})", filtered.len(), from_height, to_height);
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
                                                    println!("P2P: Synced {} blocks", blocks.len());
                                                    for block in blocks {
                                                        let h = block.header.height;
                                                        match Self::validate_and_apply_block(&block, state, &mut chain_store) {
                                                            Ok(()) => {
                                                                Self::accept_block(&block, &mut self.swarm);
                                                            }
                                                            Err(e) => {
                                                                println!("P2P: Sync block #{} rejected: {}", h, e);
                                                            }
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

                // Auto-mine every ~10 seconds if mine mode is enabled
                _ = async { if mine {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                } else {
                    futures::future::pending::<()>().await;
                }} => {
                    if mine {
                        let height = chain_store.chain_tip_height();
                        let prev_hash = chain_store.chain_tip_hash();
                        self.mine_and_gossip(prev_hash, height, state, &mut chain_store).await;
                    }
                }
            }

            // Periodic UTXO set save (every ~30s to prevent state loss on crash)
            if last_state_save.elapsed() > std::time::Duration::from_secs(30) {
                if let Err(e) = crate::store::save_utxo_set(state) {
                if let Err(e) = crate::store::save_chain_store(&chain_store) {
                    eprintln!("P2P: Chain store save failed: {}", e);
                }
                    eprintln!("P2P: State save failed: {}", e);
                }
                last_state_save = std::time::Instant::now();
            }
        }
    }

    /// Mine a block and gossip it to peers
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

                // Persist locally
                if let Err(e) = crate::store::save_block(&block) {
                    eprintln!("P2P: Save error: {}", e);
                    return;
                }
                println!("P2P: Mined block #{} hash={:x}..", h, hash[0]);

                // Add to chain store with BlockDiff for reorg safety
                if chain_store.get_block(&hash).is_none() {
                    let _ = chain_store.add_block_with_diff(block.clone(), diff);
                    chain_store.set_chain_tip(&hash).ok();
                }

                // Gossip to peers
                if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block)) {
                    self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
                    println!("P2P: Gossiped block #{}", h);
                }
            }
            Err(e) => println!("P2P: Mining failed: {}", e),
        }
    }

    pub fn gossip_block(&mut self, block: &Block) {
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
        }
    }

    pub fn peer_count(&self) -> usize { self.peers.len() }
}
