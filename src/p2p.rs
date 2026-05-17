use futures::StreamExt;
use libp2p::{
    identity, PeerId, Multiaddr, Swarm,
    noise, yamux, tcp,
    swarm::{SwarmEvent, NetworkBehaviour, Config as SwarmConfig},
    request_response::{self, ProtocolSupport},
    gossipsub::{self, MessageAuthenticity, MessageId, IdentTopic as Topic},
    mdns,
    Transport,
};
use serde::{Serialize, Deserialize};
use sha3::Digest;
use std::time::Duration;

use crate::block::{Block, Transaction};

const GOSSIP_TOPIC: &str = "ewatts-blocks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    BlockRequest { from_height: u64, to_height: u64 },
    BlockResponse { blocks: Vec<Block> },
    NewTransaction(Transaction),
    NewBlock(Block),
}

// Use JSON codec for request-response (requires Serialize + Deserialize)
type SyncBehaviour = request_response::json::Behaviour<P2pMessage, P2pMessage>;

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "P2pEvent")]
pub struct EwattsBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::Behaviour<PeerId>,
    pub block_sync: SyncBehaviour,
}

#[derive(Debug)]
pub enum P2pEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    BlockSync(request_response::Event<P2pMessage, P2pMessage>),
}

impl From<gossipsub::Event> for P2pEvent {
    fn from(e: gossipsub::Event) -> Self { P2pEvent::Gossipsub(e) }
}
impl From<mdns::Event> for P2pEvent {
    fn from(e: mdns::Event) -> Self { P2pEvent::Mdns(e) }
}
impl From<request_response::Event<P2pMessage, P2pMessage>> for P2pEvent {
    fn from(e: request_response::Event<P2pMessage, P2pMessage>) -> Self { P2pEvent::BlockSync(e) }
}

pub struct P2pNode {
    pub peer_id: PeerId,
    swarm: Swarm<EwattsBehaviour>,
    peers: Vec<PeerId>,
}

impl P2pNode {
    pub async fn new(listen_addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
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

        let m = mdns::Behaviour::new(mdns::Config::default(), peer_id)?;

        let mut block_sync = SyncBehaviour::new(
            [(StreamProtocol::new("/ewatts/block-sync/1"), ProtocolSupport::Full)],
            request_response::Config::default().with_request_timeout(Duration::from_secs(30)),
        );

        let behaviour = EwattsBehaviour { gossipsub, mdns: m, block_sync };
        let config = SwarmConfig::with_tokio_executor();
        let mut swarm = Swarm::new(transport, behaviour, peer_id, config);
        swarm.listen_on(listen_addr.parse()?)?;

        Ok(P2pNode { peer_id, swarm, peers: vec![] })
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("P2P: Listening on {address}");
                        }
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::Event::Discovered(list))) => {
                            for (peer_id, addr) in list {
                                if peer_id != *self.swarm.local_peer_id() {
                                    println!("P2P: Discovered {peer_id} at {addr}");
                                    self.swarm.dial(addr).ok();
                                    self.peers.push(peer_id);
                                }
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::Event::Expired(list))) => {
                            for (peer_id, _) in list {
                                self.peers.retain(|p| p != &peer_id);
                            }
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

    pub fn dial_peer(&mut self, addr: Multiaddr) { self.swarm.dial(addr).ok(); }
    pub fn peer_count(&self) -> usize { self.peers.len() }
}
