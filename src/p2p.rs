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

        // Connect to bootstrap peer if provided
        if let Some(addr) = bootstrap {
            swarm.dial(addr).ok();
        }

        Ok(P2pNode { peer_id, swarm, peers: HashSet::new() })
    }

    pub fn add_known_peer(&mut self, addr: Multiaddr) {
        self.swarm.dial(addr).ok();
    }

    pub async fn run(&mut self) {
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
                        }
                        SwarmEvent::ConnectionClosed { peer_id, .. } => {
                            self.peers.remove(&peer_id);
                        }
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(_) => {
                                        println!("P2P: Gossip block from {propagation_source}");
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
                                            if let P2pMessage::BlockRequest { .. } = request {
                                                let blocks = crate::store::load_blocks().unwrap_or_default();
                                                let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                                    channel, P2pMessage::BlockResponse { blocks },
                                                );
                                            }
                                        }
                                        request_response::Message::Response { response, .. } => {
                                            if let P2pMessage::BlockResponse { blocks } = response {
                                                println!("P2P: Synced {} blocks", blocks.len());
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
