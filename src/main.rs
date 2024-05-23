use clap::{Parser, ValueEnum};
use core::str;
use env_logger::Builder;
use etherparse::Ipv4HeaderSlice;
use ipnet::Ipv4Net;
use log::{debug, error, info, LevelFilter};
use std::error::Error;
use std::iter::repeat;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::signal;

use crate::codificar::*;
pub mod codificar;
use crate::route::*;
pub mod route;
use crate::device_tun::*;
pub mod device_tun;

const UDP_BUFFER_SIZE: usize = 1024 * 200; // 17kb
const DEFAULT_PORT: &str = "1714";
const DEFAULT_IFACE: &str = "rtun0";
const DEFAULT_SERVER_ADDRESS: &str = "10.9.0.1/24";
const DEFAULT_CLIENT_ADDRESS: &str = "10.9.0.2/24";
const KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const IV: &str = "010203040506";

#[derive(Parser)]
#[command(version = "0.0.1", about, long_about = None)]

struct Cli {
    #[arg(value_enum, short, long)]
    mode: Mode,
    #[arg(short, long, default_value = DEFAULT_PORT)]
    port: u16,
    #[arg(short = 'o', long, default_value = "")]
    host: String,
    #[arg(short, long, default_value = DEFAULT_IFACE)]
    iface: String,
    #[arg(short, long, default_value = "")]
    address: String,
    #[arg(short, long, default_value = "")]
    gateway: String,
    #[arg(value_enum, short, long = "loglevel", default_value = "Error")]
    log_level: LevelFilter,
    #[arg(short, long, default_value = KEY)]
    key: String,
    #[arg(long, default_value = IV)]
    iv: String,
}
#[derive(Clone, ValueEnum)]
enum Mode {
    Server,
    Client,
}

// Node
struct Node {
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    key: [u8; MAX_KEY_SIZE],
    iv: [u8; MAX_IV_SIZE],
    peer: SocketAddr,
    route: Route,
}

impl Node {
    fn clone(&self) -> Self {
        Node {
            socket: self.socket.clone(),
            tun: self.tun.clone(),
            key: self.key,
            iv: self.iv,
            peer: self.peer,
            route: self.route.clone(),
        }
    }

    async fn receive_socket_send_tun(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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

    async fn receive_tun_send_socket(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
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

    async fn run(&self) -> Result<(), Box<dyn Error>> {
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

async fn server_mode(
    port: u16,
    key: &[u8],
    iv: &[u8],
    iface: &str,
    address: &str,
) -> Result<(), Box<dyn Error>> {
    let localhost_ip = IpAddr::from_str("0.0.0.0").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let socket = UdpSocket::bind(server).await?;
    let socket_arc = Arc::new(socket);
    let tun = create_tun(iface, &address, 1500).await?;

    debug!("key {:?}", key);

    let key: [u8; MAX_KEY_SIZE] = key_to_array(key).unwrap();
    let iv: [u8; MAX_IV_SIZE] = iv_to_array(iv).unwrap();

    let server = Node {
        socket: socket_arc,
        tun,
        key,
        iv,
        peer: SocketAddr::new(localhost_ip, 0),
        route: Route::new(),
    };
    server.run().await?;

    Ok(())
}

async fn client_mode(
    server_ip: &str,
    key: &[u8],
    iv: &[u8],
    port: u16,
    gateway_ip: &str,
    iface: &str,
    address: &str,
) -> Result<(), Box<dyn Error>> {
    let remote_addr = IpAddr::from_str(server_ip).unwrap();
    let remote_server = SocketAddr::new(remote_addr, port);

    let local_addr: SocketAddr = if remote_addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()?;

    let socket = UdpSocket::bind(local_addr).await?;
    socket.connect(remote_server).await?;

    let tun = create_tun(iface, address, 1500).await?;

    let key: [u8; MAX_KEY_SIZE] = key_to_array(key).unwrap();
    let iv: [u8; MAX_IV_SIZE] = iv_to_array(iv).unwrap();

    let route: Route = Route::new();
    let remote_tun_server: Ipv4Addr = gateway_ip.parse().unwrap();
    let remote_peer_net = Ipv4Net::new(remote_tun_server, 24).unwrap();
    route.add_net_to_hashmap(remote_server, remote_peer_net, key, iv);

    let socket_arc = Arc::new(socket);
    let socket_clone = socket_arc.clone();

    let client = Node {
        socket: socket_clone,
        tun,
        key,
        iv,
        peer: remote_server,
        route,
    };

    client.run().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    Builder::new().filter(None, cli.log_level).init();

    let key = &hex_to_bytes(&cli.key)[..];
    let iv = &hex_to_bytes(&cli.iv)[..];
    match cli.mode {
        Mode::Server => {
            let address: String = if cli.address.is_empty() {
                DEFAULT_SERVER_ADDRESS.to_owned()
            } else {
                cli.address
            };
            info!(
                "Server dev({} - {}) in port {} started",
                cli.iface, address, cli.port
            );
            let _ = server_mode(cli.port, key, iv, &cli.iface, &address).await;
        }
        Mode::Client => {
            let address: String = if cli.address.is_empty() {
                DEFAULT_CLIENT_ADDRESS.to_owned()
            } else {
                cli.address
            };

            if cli.host.is_empty() {
                error!("host is required for client mode");
                std::process::exit(1);
            }
            if cli.gateway.is_empty() {
                error!("gateway is required for client mode");
                std::process::exit(2);
            }
            info!(
                "Client dev({} - {}), connect to {}:{} gw {}",
                &cli.iface, &address, cli.host, cli.port, cli.gateway
            );
            let _ = client_mode(
                &cli.host,
                key,
                iv,
                cli.port,
                &cli.gateway,
                &cli.iface,
                &address,
            )
            .await;
        }
    }
}
