use futures::StreamExt;
use libp2p::{
    core::upgrade,
    noise, yamux,
    identity, PeerId, Multiaddr,
    swarm::{SwarmEvent, SwarmBuilder, NetworkBehaviour},
    mplex,
    request_response::{self, ProtocolSupport, RequestResponse, RequestResponseEvent, RequestResponseMessage},
    gossipsub::{self, Gossipsub, GossipsubEvent, MessageAuthenticity, MessageId, TopicHash},
    mdns::{self, Mdns, MdnsEvent},
    StreamProtocol,
};
use serde::{Serialize, Deserialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::block::Block;
use crate::state::Transaction;

// ─── Protocol IDs ───────────────────────────────────────────────────────────

const BLOCK_SYNC_PROTOCOL: StreamProtocol = StreamProtocol::new("/ewatts/block-sync/1");
const TX_PROPAGATION_PROTOCOL: StreamProtocol = StreamProtocol::new("/ewatts/tx/1");
const GOSSIP_TOPIC: &str = "ewatts-blocks";

// ─── Message Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum P2pMessage {
    /// Request blocks from peer (range)
    BlockRequest {
        from_height: u64,
        to_height: u64,
    },
    /// Response with blocks
    BlockResponse {
        blocks: Vec<Block>,
    },
    /// New transaction announcement (gossip)
    NewTransaction(Transaction),
    /// New block announcement (gossip)
    NewBlock(Block),
    /// Peer discovery query
    PeerQuery,
    /// Peer discovery response
    PeerList(Vec<Multiaddr>),
}

// ─── Network Behaviour ──────────────────────────────────────────────────────

#[derive(NetworkBehaviour)]
#[behaviour(out_event = "P2pEvent")]
pub struct EwattsBehaviour {
    pub gossipsub: Gossipsub,
    pub mdns: Mdns,
    pub block_sync: RequestResponse<P2pMessage>,
    pub tx_prop: RequestResponse<P2pMessage>,
}

#[derive(Debug)]
pub enum P2pEvent {
    Gossipsub(GossipsubEvent),
    Mdns(MdnsEvent),
    BlockSync(RequestResponseEvent<P2pMessage, P2pMessage>),
    TxProp(RequestResponseEvent<P2pMessage, P2pMessage>),
}

impl From<GossipsubEvent> for P2pEvent {
    fn from(e: GossipsubEvent) -> Self { P2pEvent::Gossipsub(e) }
}
impl From<MdnsEvent> for P2pEvent {
    fn from(e: MdnsEvent) -> Self { P2pEvent::Mdns(e) }
}
impl From<RequestResponseEvent<P2pMessage, P2pMessage>> for P2pEvent {
    fn from(e: RequestResponseEvent<P2pMessage, P2pMessage>) -> Self { P2pEvent::BlockSync(e) }
}

// ─── Node ───────────────────────────────────────────────────────────────────

pub struct P2pNode {
    pub peer_id: PeerId,
    swarm: libp2p::Swarm<EwattsBehaviour>,
    event_rx: mpsc::Receiver<P2pEvent>,
    peers: Vec<PeerId>,
}

impl P2pNode {
    pub async fn new(listen_addr: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let local_key = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(local_key.public());

        // Transport
        let transport = libp2p::development_transport(local_key.clone()).await?;

        // Gossipsub
        let message_id_fn = |msg: &gossipsub::Message| {
            MessageId::from(&msg.data[..std::cmp::min(msg.data.len(), 32)])
        };
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(5))
            .message_id_fn(message_id_fn)
            .build()
            .map_err(|e| format!("Gossipsub config: {e}"))?;
        let gossipsub = Gossipsub::new(MessageAuthenticity::Signed(local_key.clone()), gossipsub_config)
            .map_err(|e| format!("Gossipsub: {e}"))?;

        // mDNS
        let mdns = Mdns::new(mdns::Config::default()).await?;

        // Request/Response protocols
        let block_sync = RequestResponse::new(
            request_response::Config::default().with_request_timeout(Duration::from_secs(30)),
            ProtocolSupport::Full,
        );
        let tx_prop = RequestResponse::new(
            request_response::Config::default().with_request_timeout(Duration::from_secs(10)),
            ProtocolSupport::Full,
        );

        let behaviour = EwattsBehaviour { gossipsub, mdns, block_sync, tx_prop };

        let mut swarm = SwarmBuilder::with_existing_identity(local_key)
            .with_tokio()
            .with_other_enclose(|k| transport)(move |k| behaviour)?
            .build();

        swarm.listen_on(listen_addr.parse()?)?;
        let (event_tx, event_rx) = mpsc::channel::<P2pEvent>(256);

        Ok(P2pNode { peer_id, swarm, event_rx, peers: vec![] })
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    match event {
                        SwarmEvent::NewListenAddr { address, .. } => {
                            println!("P2P: Listening on {address}");
                        }
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::MdnsEvent::Discovered(peers))) => {
                            for (peer_id, addr) in peers {
                                println!("P2P: Discovered {peer_id} at {addr}");
                                self.swarm.dial(addr).ok();
                                self.peers.push(peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::Mdns(mdns::MdnsEvent::Expired(peers))) => {
                            for (peer_id, _) in peers {
                                self.peers.retain(|p| p != &peer_id);
                            }
                        }
                        SwarmEvent::Behaviour(P2pEvent::BlockSync(event)) => {
                            self.handle_block_sync_event(event).await;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    async fn handle_block_sync_event(&mut self, event: RequestResponseEvent<P2pMessage, P2pMessage>) {
        match event {
            RequestResponseEvent::Message { peer, message } => {
                match message {
                    RequestResponseMessage::Request { request_id, request, channel, .. } => {
                        if let P2pMessage::BlockRequest { from_height, to_height } = request {
                            // Load blocks from store and respond
                            if let Ok(blocks) = crate::store::load_blocks() {
                                let filtered: Vec<Block> = blocks.into_iter()
                                    .filter(|b| {
                                        // TODO: extract height from block
                                        true // placeholder
                                    })
                                    .collect();
                                let _ = self.swarm.behaviour_mut().block_sync.send_response(
                                    channel,
                                    P2pMessage::BlockResponse { blocks: filtered },
                                );
                            }
                        }
                    }
                    RequestResponseMessage::Response { response, .. } => {
                        if let P2pMessage::BlockResponse { blocks } = response {
                            for block in blocks {
                                // TODO: validate and apply block
                                // crate::state::apply_block(...)
                                println!("P2P: Received block");
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn dial_peer(&mut self, addr: Multiaddr) {
        self.swarm.dial(addr).ok();
    }

    pub fn gossip_block(&mut self, block: &Block) {
        let topic = gossipsub::IdentTopic::new(GOSSIP_TOPIC);
        let msg = P2pMessage::NewBlock(block.clone());
        if let Ok(data) = serde_json::to_vec(&msg) {
            self.swarm.behaviour_mut().gossipsub.publish(topic, data).ok();
        }
    }

    pub fn gossip_transaction(&mut self, tx: &Transaction) {
        let topic = gossipsub::IdentTopic::new(GOSSIP_TOPIC);
        let msg = P2pMessage::NewTransaction(tx.clone());
        if let Ok(data) = serde_json::to_vec(&msg) {
            self.swarm.behaviour_mut().gossipsub.publish(topic, data).ok();
        }
    }

    pub fn request_blocks(&mut self, peer: PeerId, from: u64, to: u64) {
        self.swarm.behaviour_mut().block_sync.send_request(
            &peer,
            P2pMessage::BlockRequest { from_height: from, to_height: to },
        );
    }

    pub fn peer_count(&self) -> usize { self.peers.len() }
}
