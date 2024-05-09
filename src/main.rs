use clap::{Parser, ValueEnum};
use env_logger::Builder;
use etherparse::Ipv4HeaderSlice;
use ipnet::Ipv4Net;
use log::{debug, error, info, LevelFilter};
use std::collections::HashMap;
use std::error::Error;
use std::net::Ipv4Addr;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use tokio::net::UdpSocket;
use tokio::signal;
use tokio_tun::Tun;

const UDP_BUFFER_SIZE: usize = 17480; // 17kb

#[derive(Parser)]
#[command(version = "0.0.1", about, long_about = None)]

struct Cli {
    #[arg(value_enum, short, long)]
    mode: Mode,
    #[arg(short, long, default_value = "1714")]
    port: u16,
    #[arg(short = 'o', long, default_value = "")]
    host: String,
    #[arg(short, long, default_value = "rtun0")]
    iface: String,
    #[arg(short, long, default_value = "")]
    address: String,
    #[arg(short, long, default_value = "")]
    gateway: String,
    #[arg(value_enum, short, long = "loglevel", default_value = "Error")]
    log_level: LevelFilter,
}
#[derive(Clone, ValueEnum)]
enum Mode {
    Server,
    Client,
}

async fn create_tun(iface: &str, ipv4: &str, mtu: i32) -> tokio_tun::Tun {
    let net: Ipv4Net = ipv4.parse().unwrap();

    let tun = Tun::builder()
        .name(iface)
        .tap(false)
        .packet_info(false)
        .mtu(mtu)
        .up()
        .address(net.addr())
        .broadcast(Ipv4Addr::BROADCAST)
        .netmask(net.netmask())
        .try_build()
        .unwrap();
    info!("Tun interface created: {:?}, with IP {}", tun.name(), ipv4);
    return tun;
}
struct Node {
    socket: Arc<UdpSocket>,
    tun: tokio_tun::Tun,
    peer: SocketAddr,
    peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>,
}

fn get_peer_from_hashmap(
    peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>,
    destination_addrs: IpAddr,
) -> Option<SocketAddr> {
    let peers = peers.lock().unwrap();
    debug!("Get IP Address '{:?}' from HashMap (get_peer_from_hashmap)", destination_addrs);
    if peers.contains_key(&destination_addrs) {
        // get the socket from the hashmap
        let socket_addr = peers.get(&destination_addrs).unwrap();
        let socket_addr = **socket_addr;
        debug!("Existing Socket ({:?}) for this IP Address in HashMap (get_peer_from_hashmap)", socket_addr);
        return Some(socket_addr);
    } else {
        debug!("NO Existing IP Address in HashMap (get_peer_from_hashmap)");
        return None;
    }
}

// Work with the HashMap

fn show_hashmap(peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>) {
    let peers = peers.lock().unwrap();
    for (k, v) in peers.iter() {
        debug!("Element in HashMap: {:?} {:?}", k, v);
    }
}

fn add_peer_to_hashmap(
    peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>,
    peer: SocketAddr,
    source_addrs: Ipv4Addr,
) {
    let mut peers = peers.lock().unwrap();

    // If peer is not in the hashmap, add it
    debug!("Peer is in hashmap: {:?}", peers.contains_key(&peer.ip()));
    if !peers.contains_key(&peer.ip()) {
        debug!("Adding peer: {:?}", peer);
        let socket_arc = Arc::new(peer);
        peers.insert(IpAddr::V4(source_addrs), socket_arc);
    }
}

async fn receive_tun_send_socket(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    peer: Arc<SocketAddr>,
    peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        let size = tun.recv(&mut buf).await?;

        debug!(
            "receive_tun_send_socket: Received from tun {}/{} bytes from tun sent to: {}",
            size, UDP_BUFFER_SIZE, peer
        );
        
        show_hashmap(peers.clone());
        
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
                match get_peer_from_hashmap(peers.clone(), destination_addrs) {
                    Some(socket_addr) => {
                        // Here: we will encrypt the buffer
                        let _ = socket.send_to(&buf[..size], socket_addr).await;
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

async fn receive_socket_send_tun(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    peers: Arc<Mutex<HashMap<IpAddr, Arc<SocketAddr>>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        let (size, peer) = socket.recv_from(&mut buf).await?;

        debug!(
            "receive_socket_send_tun: Received from socket {}/{} bytes from tun sent to: {}",
            size, UDP_BUFFER_SIZE, peer
        );

        // Here: we will desencrypt the buffer

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
                add_peer_to_hashmap(peers.clone(), peer, source_addrs);
            }
        }
        debug!("receive_socket_send_tun; buffer: {:?}", &buf[..size]);
        debug!("receive_socket_send_tun: Sent to tun");
        tun.send(&buf[..size]).await?;
    }
}

impl Node {
    async fn run(self) -> Result<(), Box<dyn Error>> {
        let Node {
            socket,
            tun,
            peer,
            peers,
        } = self;

        debug!("Node started");

        // Clone udp socket
        let socket_clone = socket.clone();
        // Clone tun
        let tun_arc = Arc::new(tun);
        let tun_clone = tun_arc.clone();
        // Clone peer
        let peer_arc = Arc::new(peer);
        let peers_clone = peers.clone();

        tokio::select! {

            res = tokio::spawn(async move {
                // receive from tun and send to socket
                let _ = receive_tun_send_socket(socket, tun_arc, peer_arc, peers_clone ).await;
            }) => { res.map_err(|e| e.into()) },

            res = tokio::spawn(async move {
                // receive from socket and send to tun
                let _ = receive_socket_send_tun(socket_clone, tun_clone, peers).await;
            }) => { res.map_err(|e| e.into()) },

            res = signal::ctrl_c() => {
                res.map_err(|e| e.into())
            }
        }
    }
}

async fn server_mode(port: u16, iface: &str, address: &str) -> Result<(), Box<dyn Error>> {
    let localhost_ip = IpAddr::from_str("0.0.0.0").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let socket = UdpSocket::bind(server).await?;
    let socket_arc = Arc::new(socket);
    let tun = create_tun(iface, &address, 1500).await;

    let server = Node {
        socket: socket_arc,
        tun,
        peer: SocketAddr::new(localhost_ip, 0),
        peers: Arc::new(Mutex::new(HashMap::new())),
    };
    server.run().await?;

    Ok(())
}

async fn client_mode(server_ip: &str, port: u16, gateway_ip: &str, iface: &str, address: &str) -> Result<(), Box<dyn Error>> {
    let remote_addr = IpAddr::from_str(server_ip).unwrap();
    let remote_server = SocketAddr::new(remote_addr, port);
    let remote_server_arc = Arc::new(remote_server);

    let local_addr: SocketAddr = if remote_addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    }
    .parse()?;

    let socket = UdpSocket::bind(local_addr).await?;
    socket.connect(remote_server).await?;

    let tun = create_tun(iface, address, 1500).await;
    let mut peers = HashMap::new();
//    peers.insert(remote_addr, remote_server_arc);
    let remote_tun_server: Ipv4Addr = gateway_ip.parse().unwrap();
    peers.insert(IpAddr::V4(remote_tun_server), remote_server_arc);

    let socket_arc = Arc::new(socket);
    let socket_clone = socket_arc.clone();

    let client = Node {
        socket: socket_clone,
        tun,
        peer: remote_server,
        peers: Arc::new(Mutex::new(peers)),
    };

    client.run().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    
    let cli = Cli::parse();
    Builder::new().filter(None, cli.log_level).init();

    match cli.mode {
        Mode::Server => {
            let address: String = if cli.address.is_empty() { "10.9.0.1/24".to_owned() } else { cli.address };
            info!("Server dev({} - {}) in port {} started", cli.iface, address, cli.port);
            let _ = server_mode(cli.port, &cli.iface, &address).await;
        }
        Mode::Client => {
            let address: String = if cli.address.is_empty() { "10.9.0.2/24".to_owned() } else { cli.address };

            if cli.host.is_empty() {
                error!("host is required for client mode");
                std::process::exit(1);
            }
            if cli.gateway.is_empty() {
                error!("gateway is required for client mode");
                std::process::exit(2);
            }
            info!("Client dev({} - {}), connect to {}:{} gw {}", &cli.iface, &address, cli.host, cli.port, cli.gateway);
            let _ = client_mode(&cli.host, cli.port, &cli.gateway, &cli.iface, &address).await;
        }
    }
}