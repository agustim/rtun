use std::iter::repeat;
use tokio::signal;
use etherparse::Ipv4HeaderSlice;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;
use log::{debug, error};
use crate::codificar::*;
use crate::route::*;


const UDP_BUFFER_SIZE: usize = 1024 * 200; // 17kb

// Node
#[derive(Clone)]
pub struct Node {
    pub socket: Arc<UdpSocket>,
    pub tun: Arc<tokio_tun::Tun>,
    pub key: [u8; MAX_KEY_SIZE],
    pub iv: [u8; MAX_IV_SIZE],
    pub peer: SocketAddr,
    pub route: Route,
}

impl Node {

    pub async fn receive_socket_send_tun(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let Node {
            socket,
            tun,
            key,
            iv,
            peer: _,
            route,
        } = self.clone();

        let mut buf_enc = [0u8; UDP_BUFFER_SIZE];

        loop {
            let (size, peer) = socket.recv_from(&mut buf_enc).await?;

            debug!(
                "receive_socket_send_tun: Received from socket {}/{} bytes from tun sent to: {}",
                size, UDP_BUFFER_SIZE, peer
            );

            debug!("receive_socket_send_tun: size buffer_enc: {:?}", size);
            //let mut buf = [0u8; UDP_BUFFER_SIZE];
            let mut buf: Vec<u8> = repeat(0).take(size).collect();
            decrypt(&key, &iv, buf_enc[..size].to_vec(), &mut buf[..]);
            debug!("receive_socket_send_tun: size buffer_dec: {:?}", buf.len());

            match Ipv4HeaderSlice::from_slice(&buf[..size]) {
                Err(e) => {
                    error!("receive_socket_send_tun: Ignore Package with any problem in IPv4Header: {:?}", e);
                }
                Ok(value) => {
                    let source_addrs = value.source_addr();
                    let destination_addrs = value.destination_addr();
                    debug!(
                        "receive_socket_send_tun: {:?} => {:?}",
                        source_addrs, destination_addrs
                    );
                    let key: [u8; MAX_KEY_SIZE] = key_to_array(&self.key).unwrap();
                    let iv: [u8; MAX_IV_SIZE] = iv_to_array(&self.iv).unwrap();
                    route.add_peer_to_hashmap(peer, source_addrs, key, iv);
                    debug!("receive_socket_send_tun; size buffer: {:?}", size);
                    debug!("receive_socket_send_tun: Sent to tun");
                    tun.send(&buf[..size]).await?;
                }
            }
        }
    }

    pub async fn receive_tun_send_socket(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let Node {
            socket,
            tun,
            key,
            iv,
            peer,
            route,
        } = self.clone();

        let mut buf = [0u8; UDP_BUFFER_SIZE];

        loop {
            let size = tun.recv(&mut buf).await?;

            debug!(
                "receive_tun_send_socket: Received from tun {}/{} bytes from tun sent to: {}",
                size, UDP_BUFFER_SIZE, peer
            );

            route.show_hashmap();

            match Ipv4HeaderSlice::from_slice(&buf[..size]) {
                Err(e) => {
                    debug!("receive_tun_send_socket: Ignore Package with any problem in IPv4Header: {:?}", e);
                }
                Ok(value) => {
                    let destination_addrs = IpAddr::V4(value.destination_addr());
                    let source_addrs = IpAddr::V4(value.source_addr());
                    debug!(
                        "receive_tun_send_socket: source: {:?}, destination: {:?}",
                        source_addrs, destination_addrs
                    );
                    match route.get_peer_from_hashmap(destination_addrs) {
                        Some(peer_addr) => {
                            let encrypted = encrypt(&key, &iv, &buf[..size]);
                            let _ = socket.send_to(&encrypted[..], peer_addr.socket_addr).await;
                            debug!("receive_tun_send_socket: : Sent to socket");
                        }
                        None => {
                            debug!("receive_tun_send_socket: : Peer is not in hashmap");
                        }
                    }
                }
            }
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        debug!("Node started");
        // Clone self for each thread, to use in tokio::select
        let self_tun = self.clone();
        let self_socket = self.clone();

        tokio::select! {

            res = tokio::spawn(async move {
                // receive from tun and send to socket
                let _ = self_tun.receive_tun_send_socket().await;
            }) => { res.map_err(|e| e.into()) },

            res = tokio::spawn(async move {
                // receive from socket and send to tun
                let _ = self_socket.receive_socket_send_tun().await;
            }) => { res.map_err(|e| e.into()) },

            res = signal::ctrl_c() => {
                res.map_err(|e| e.into())
            }
        }
    }
}
