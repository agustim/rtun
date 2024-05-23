use crate::codificar::*;
use ipnet::Ipv4Net;
use log::debug;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Peer {
    pub socket_addr: SocketAddr,
    key: [u8; MAX_KEY_SIZE],
    iv: [u8; MAX_IV_SIZE],
}

#[derive(Debug, Clone)]
pub struct Route {
    peers: Arc<Mutex<HashMap<Ipv4Net, Arc<Peer>>>>,
}

impl Route {
    pub fn new() -> Self {
        Route {
            peers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn get_peer_from_hashmap(&self, destination_addrs: IpAddr) -> Option<Arc<Peer>> {
        let peers = self.peers.lock().unwrap();
        debug!(
            "Get IP Address '{:?}' from HashMap (get_peer_from_hashmap)",
            destination_addrs
        );
        let mut prefix = 0;
        let mut peer_ret: Option<Arc<Peer>> = None;

        for (k, v) in peers.iter() {
            debug!(
                "Element in HashMap: {:?} [{:?} {:?} {:?}]",
                k, v.socket_addr, v.key, v.iv
            );
            let destination_addrs: Ipv4Addr = match destination_addrs {
                IpAddr::V4(ip) => ip,
                _ => return None,
            };
            if k.contains(&destination_addrs) {
                debug!("Existing IP Address in HashMap (get_peer_from_hashmap)");
                if prefix <= k.prefix_len() {
                    prefix = k.prefix_len();
                    peer_ret = Some(v.clone());
                }
            }
        }
        if let Some(peer_ret) = peer_ret {
            return Some(peer_ret);
        } else {
            debug!("NO Existing IP Address in HashMap (get_peer_from_hashmap)");
            return None;
        }
    }

    // Work with the HashMap

    pub fn show_hashmap(&self) {
        let peers = self.peers.lock().unwrap();
        for (k, v) in peers.iter() {
            debug!(
                "Element in HashMap: {:?} [{:?} {:?} {:?}]",
                k, v.socket_addr, v.key, v.iv
            );
        }
    }
    pub fn add_net_to_hashmap(
        &self,
        peer: SocketAddr,
        net: Ipv4Net,
        key: [u8; MAX_KEY_SIZE],
        iv: [u8; MAX_IV_SIZE],
    ) {
        let mut peers = self.peers.lock().unwrap();

        // If peer is not in the hashmap, add it
        debug!("Peer is in hashmap: {:?}", peers.contains_key(&net));
        debug!("Adding or Update peer: {:?}", peer);
        let insert_peer = Arc::new(Peer {
            socket_addr: peer,
            key,
            iv,
        });
        peers.insert(net, insert_peer);
    }

    pub fn add_peer_to_hashmap(
        &self,
        peer: SocketAddr,
        source_addrs: Ipv4Addr,
        key: [u8; MAX_KEY_SIZE],
        iv: [u8; MAX_IV_SIZE],
    ) {
        let peer_net = Ipv4Net::new(source_addrs, 32).unwrap();
        self.add_net_to_hashmap(peer, peer_net, key, iv);
    }
}
