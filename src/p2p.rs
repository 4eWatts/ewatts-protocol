use futures::StreamExt;
use libp2p::{
    identity, PeerId, Multiaddr, Swarm, SwarmEvent,
    noise, yamux, tcp, dns,
    request_response::{self, ProtocolSupport, RequestResponse, RequestResponseEvent, RequestResponseMessage, OutboundRequestId},
    gossipsub::{self, Gossipsub, MessageAuthenticity, MessageId, IdentTopic as Topic},
    mdns::{self, Mdns, MdnsEvent},
    StreamProtocol,
    swarm::NetworkBehaviour,
};
use serde::{Serialize, Deserialize};
use std::time::Duration;

use crate::block::{Block, Transaction};

// ─── Protocol IDs ───────────────────────────────────────────────────────────

const GOSSIP_TOPIC: &str = "ewatts-blocks";

// ─── Message Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    BlockRequest { from_height: u64, to_height: u64 },
    BlockResponse { blocks: Vec<Block> },
    NewTransaction(Transaction),
    NewBlock(Block),
}

// ─── Network Behaviour ──────────────────────────────────────────────────────

#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "P2pEvent")]
pub struct EwattsBehaviour {
    pub gossipsub: Gossipsub,
    pub mdns: Mdns,
    pub block_sync: RequestResponse<P2pMessage, P2pMessage>,
}

#[derive(Debug)]
pub enum P2pEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    BlockSync(RequestResponseEvent<P2pMessage, P2pMessage>),
}

impl From<gossipsub::Event> for P2pEvent {
    fn from(e: gossipsub::Event) -> Self { P2pEvent::Gossipsub(e) }
}
impl From<mdns::Event> for P2pEvent {
    fn from(e: mdns::Event) -> Self { P2pEvent::Mdns(e) }
}
impl From<RequestResponseEvent<P2pMessage, P2pMessage>> for P2pEvent {
    fn from(e: RequestResponseEvent<P2pMessage, P2pMessage>) -> Self { P2pEvent::BlockSync(e) }
}

// ─── Node ───────────────────────────────────────────────────────────────────

pub struct P2pNode {
    pub peer_id: PeerId,
    swarm: Swarm<EwattsBehaviour>,
    peers: Vec<PeerId>,
}

impl P2pNode {
    pub async fn new(listen_addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(local_key.public());

        // Transport: TCP + DNS + Noise + Yamux
        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&local_key)?)
            .multiplex(yamux::Config::default())
            .boxed();

        // Gossipsub
        let message_id_fn = |msg: &gossipsub::Message| {
            let mut hasher = sha3::Keccak256::default();
            hasher.update(&msg.data);
            MessageId::from(hasher.finalize().to_vec())
        };
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(5))
            .message_id_fn(message_id_fn)
            .build()?;
        let gossipsub = Gossipsub::new(MessageAuthenticity::Signed(local_key.clone()), gossipsub_config)?;
        let topic = Topic::new(GOSSIP_TOPIC);
        gossipsub.subscribe(&topic)?;

        // mDNS
        let mdns = Mdns::new(mdns::Config::default()).await?;

        // Request/Response
        let block_sync = RequestResponse::new(
            request_response::Config::default()
                .with_request_timeout(Duration::from_secs(30)),
            ProtocolSupport::Full,
        );

        let behaviour = EwattsBehaviour { gossipsub, mdns, block_sync };

        let mut swarm = Swarm::new(transport, behaviour, peer_id);
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
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::Event::Discovered(peers))) => {
                            for (peer_id, addr) in peers {
                                println!("P2P: Discovered {peer_id} at {addr}");
                                self.swarm.dial(addr).ok();
                                self.peers.push(peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::Event::Expired(peers))) => {
                            for (peer_id, _) in peers {
                                self.peers.retain(|p| p != &peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::Gossipsub(gossipsub::Event::Message {
                            propagation_source, message, ..
                        })) => {
                            if let Ok(msg) = serde_json::from_slice::<P2pMessage>(&message.data) {
                                match msg {
                                    P2pMessage::NewBlock(block) => {
                                        println!("P2P: Gossip received block from {propagation_source}");
                                        // TODO: validate and store block
                                    }
                                    P2pMessage::NewTransaction(tx) => {
                                        println!("P2P: Gossip received tx from {propagation_source}");
                                        // TODO: validate and mempool
                                    }
                                    _ => {}
                                }
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::BlockSync(event)) => {
                            match event {
                                RequestResponseEvent::Message { peer: _, message } => {
                                    match message {
                                        RequestResponseMessage::Request { request, channel, .. } => {
                                            if let P2pMessage::BlockRequest { from_height, to_height } = request {
                                                let blocks = crate::store::load_blocks().unwrap_or_default()
                                                    .into_iter()
                                                    .filter(|_| true) // TODO: filter by height
                                                    .collect();
                                                let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                                    channel, P2pMessage::BlockResponse { blocks },
                                                );
                                            }
                                        }
                                        RequestResponseMessage::Response { response, .. } => {
                                            if let P2pMessage::BlockResponse { blocks } = response {
                                                println!("P2P: Received {} blocks via sync", blocks.len());
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

    pub fn dial_peer(&mut self, addr: Multiaddr) {
        self.swarm.dial(addr).ok();
    }

    pub fn gossip_block(&mut self, block: &Block) {
        let topic = Topic::new(GOSSIP_TOPIC);
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewBlock(block.clone())) {
            self.swarm.behaviour_mut().gossipsub.publish(topic, data).ok();
        }
    }

    pub fn gossip_transaction(&mut self, tx: &Transaction) {
        let topic = Topic::new(GOSSIP_TOPIC);
        if let Ok(data) = serde_json::to_vec(&P2pMessage::NewTransaction(tx.clone())) {
            self.swarm.behaviour_mut().gossipsub.publish(topic, data).ok();
        }
    }

    pub fn request_blocks(&mut self, peer: PeerId, from: u64, to: u64) {
        self.swarm.behaviour_mut().block_sync.send_request(
            &peer, P2pMessage::BlockRequest { from_height: from, to_height: to },
        );
    }

    pub fn peer_count(&self) -> usize { self.peers.len() }
}
