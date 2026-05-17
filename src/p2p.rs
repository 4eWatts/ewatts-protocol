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

    /// Mine a block and gossip it to peers
    pub async fn mine_and_gossip(&mut self, prev_hash: [u8; 32], height: u64, state: &mut crate::state::UtxoSet) {
        match mine_block(prev_hash, height, state) {
            Ok(block) => {
                let hash = block.header.hash();
                let h = block.header.height;
                if let Err(e) = crate::store::save_block(&block) {
                    println!("P2P: Save error: {}", e); return;
                }
                println!("P2P: Mined block #{} hash={}", h, hex::encode(&hash[..8]));

                // Gossip to peers
                if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block)) {
                    self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
                    println!("P2P: Gossiped block #{}", h);
                }
            }
            Err(e) => println!("P2P: Mining failed: {}", e),
        }
    }

    pub async fn run(&mut self, mine: bool, state: &mut crate::state::UtxoSet) {
        let mut last_mine = std::time::Instant::now();

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
                            let our_blocks = crate::store::load_blocks().unwrap_or_default();
                            let from = our_blocks.len() as u64;
                            self.swarm.behaviour_mut().block_sync.send_request(
                                &peer_id, P2pMessage::BlockRequest { from_height: from, to_height: from + 100 },
                            );
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            self.peers.remove(&peer_id);
                        }
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(block) => {
                                        let h = block.header.height;
                                        println!("P2P: Gossip block #{} from {propagation_source}", h);

                                        // Validate and store if we don't have it
                                        if let Ok(blocks) = crate::store::load_blocks() {
                                            if h <= blocks.len() as u64 {
                                                // Skip if we already have this height
                                                return;
                                            }
                                        }
                                        if let Err(e) = crate::store::save_block(&block) {
                                            println!("P2P: Store error: {}", e);
                                        }
                                    }
                                    P2pMessage::NewTransaction(_) => {
                                        println!("P2P: Gossip tx from {propagation_source}");
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
                                                        if let Err(e) = crate::store::save_block(&block) {
                                                            println!("P2P: Store error: {}", e);
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
                        let blocks = crate::store::load_blocks().unwrap_or_default();
                        let height = blocks.len() as u64;
                        let prev_hash = if height == 0 { [0u8;32] } else { blocks.last().unwrap().header.hash() };
                        self.mine_and_gossip(prev_hash, height, state).await;
                    }
                }
            }
        }
    }

    pub fn gossip_block(&mut self, block: &Block) {
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            self.swarm.behaviour_mut().gossipsub.publish(Topic::new(GOSSIP_TOPIC), data).ok();
        }
    }

    pub fn peer_count(&self) -> usize { self.peers.len() }
}
