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
// use base64;
use crypto;
use rustc_serialize::hex::FromHex;
use std::iter::repeat;
use crypto::symmetriccipher::SynchronousStreamCipher;
use core::str;

const UDP_BUFFER_SIZE: usize = 1024 * 200; // 17kb
const DEFAULT_PORT: &str = "1714";
const DEFAULT_IFACE: &str = "rtun0";
const DEFAULT_SERVER_ADDRESS: &str = "10.9.0.1/24";
const DEFAULT_CLIENT_ADDRESS: &str = "10.9.0.2/24";
const MAX_KEY_SIZE: usize = 32;
const MAX_IV_SIZE: usize = 12;
const KEY : &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const IV : &str = "010203040506";

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

//Segur que hi ha una forma més correcte!!
//Convert &[u8] to [u8;MAX_KEY_SIZE]
fn key_to_array(key: &[u8]) -> Result<[u8;MAX_KEY_SIZE], &'static str> {
    let mut key_array = [0u8;MAX_KEY_SIZE];
    let key_len = key.len();

    if key_len > MAX_KEY_SIZE {
        return Err("Key size is too big");
    }
    for i in 0..key_len {
        key_array[i] = key[i];
    }
    // Fill the rest with zero
    for i in key_len..MAX_KEY_SIZE {
        key_array[i] = 0;
    }
    return Ok(key_array);
}
//Convert &[u8] to [u8;MAX_IV_SIZE]
fn iv_to_array(k: &[u8]) -> Result<[u8;MAX_IV_SIZE], &'static str> {
    let mut ret_array = [0u8;MAX_IV_SIZE];
    let k_len = k.len();

    if k_len > MAX_IV_SIZE {
        return Err("IV size is too big");
    }
    for i in 0..k_len {
        ret_array[i] = k[i];
    }
    // Fill the rest with zero
    for i in k_len..MAX_IV_SIZE {
        ret_array[i] = 0;
    }
    return Ok(ret_array);
}


// Encrypt and Decrypt functions
fn hex_to_bytes(s: &str) -> Vec<u8> {
    match s.from_hex() {
        Ok(r) => return r,
        Err(e) => {
            error!("Error: {:?}, this string contains some different character to [0-9a-f]. Generate key with all zero.", e);
            //return vector with 0
            return repeat(0).take(0).collect();
        },
    };
}

fn encrypt (key: &[u8], iv: &[u8], msg: &[u8]) -> Vec<u8> {
    let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
    let mut output: Vec<u8> = repeat(0).take(msg.len()).collect();
    debug!("Encrypting message");
    c.process(&msg[..], &mut output[..]);
    return output;
}


fn decrypt (key: &[u8], iv: &[u8], msg: Vec<u8>, ret: &mut [u8]) {
    let mut c = crypto::chacha20::ChaCha20::new(&key, iv);
    let mut output = msg;
    let mut newoutput: Vec<u8> = repeat(0).take(output.len()).collect();
    debug!("Decrypting message");
    c.process(&mut output[..], &mut newoutput[..]);
    ret.copy_from_slice(&newoutput[..]);
}

// Peer
struct Peer {
    socket_addr: SocketAddr,
    key: [u8;MAX_KEY_SIZE],
    iv: [u8;MAX_IV_SIZE],
}

// Node
struct Node {
    socket: Arc<UdpSocket>,
    tun: tokio_tun::Tun,
    key: [u8;MAX_KEY_SIZE],
    iv: [u8;MAX_IV_SIZE],
    peer: SocketAddr,
    peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>,
}

fn get_peer_from_hashmap(
    peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>,
    destination_addrs: IpAddr,
) -> Option<Arc<Peer>> {
    let peers = peers.lock().unwrap();
    debug!("Get IP Address '{:?}' from HashMap (get_peer_from_hashmap)", destination_addrs);
    if peers.contains_key(&destination_addrs) {
        // get the socket from the hashmap
        let peer_addr = peers.get(&destination_addrs).unwrap();
        debug!("Existing IP Address in HashMap (get_peer_from_hashmap)");
        return Some(peer_addr.clone());
    } else {
        debug!("NO Existing IP Address in HashMap (get_peer_from_hashmap)");
        return None;
    }
}

// Work with the HashMap

fn show_hashmap(peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>) {
    let peers = peers.lock().unwrap();
    for (k, v) in peers.iter() {
        debug!("Element in HashMap: {:?} [{:?} {:?} {:?}]", k, v.socket_addr, v.key, v.iv);
    }
}

fn add_peer_to_hashmap(
    peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>,
    peer: SocketAddr,
    source_addrs: Ipv4Addr,
    key: [u8;MAX_KEY_SIZE],
    iv: [u8;MAX_IV_SIZE],
) {
    let mut peers = peers.lock().unwrap();

    // If peer is not in the hashmap, add it
    debug!("Peer is in hashmap: {:?}", peers.contains_key(&peer.ip()));
    if !peers.contains_key(&peer.ip()) {
        debug!("Adding peer: {:?}", peer);
        let insert_peer = Arc::new(Peer {
            socket_addr: peer,
            key,
            iv,
        });
        peers.insert(IpAddr::V4(source_addrs), insert_peer);
    }
}

async fn receive_tun_send_socket(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    key: &[u8],
    iv: &[u8],
    peer: Arc<SocketAddr>,
    peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>,
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

async fn receive_socket_send_tun(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    key: &[u8],
    iv: &[u8],
    peers: Arc<Mutex<HashMap<IpAddr, Arc<Peer>>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
        decrypt(key, iv, buf_enc[..size].to_vec(), &mut buf[..]);
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
                let key:[u8;MAX_KEY_SIZE] = key_to_array(key).unwrap();
                let iv:[u8;MAX_IV_SIZE] = iv_to_array(iv).unwrap();
                add_peer_to_hashmap(peers.clone(), peer, source_addrs, key, iv);
            }
        }
        debug!("receive_socket_send_tun; size buffer: {:?}", size);
        debug!("receive_socket_send_tun: Sent to tun");
        tun.send(&buf[..size]).await?;
    }
}

impl Node {
    async fn run(self) -> Result<(), Box<dyn Error>> {
        let Node {
            socket,
            tun,
            key,
            iv,
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
                let _ = receive_tun_send_socket(socket, tun_arc, &key, &iv, peer_arc, peers_clone ).await;
            }) => { res.map_err(|e| e.into()) },

            res = tokio::spawn(async move {
                // receive from socket and send to tun
                let _ = receive_socket_send_tun(socket_clone, tun_clone, &key, &iv, peers).await;
            }) => { res.map_err(|e| e.into()) },

            res = signal::ctrl_c() => {
                res.map_err(|e| e.into())
            }
        }
    }
}

async fn server_mode(port: u16, key: &[u8], iv: &[u8], iface: &str, address: &str) -> Result<(), Box<dyn Error>> {
    let localhost_ip = IpAddr::from_str("0.0.0.0").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let socket = UdpSocket::bind(server).await?;
    let socket_arc = Arc::new(socket);
    let tun = create_tun(iface, &address, 1500).await;

    debug!("key {:?}", key);

    let key:[u8;MAX_KEY_SIZE] = key_to_array(key).unwrap();
    let iv:[u8;MAX_IV_SIZE] = iv_to_array(iv).unwrap();

    let server = Node {
        socket: socket_arc,
        tun,
        key,
        iv,
        peer: SocketAddr::new(localhost_ip, 0),
        peers: Arc::new(Mutex::new(HashMap::new())),
    };
    server.run().await?;

    Ok(())
}

async fn client_mode(server_ip: &str, key: &[u8], iv: &[u8], port: u16, gateway_ip: &str, iface: &str, address: &str) -> Result<(), Box<dyn Error>> {
    let remote_addr = IpAddr::from_str(server_ip).unwrap();
    let remote_server = SocketAddr::new(remote_addr, port);

    let local_addr: SocketAddr = if remote_addr.is_ipv4() { "0.0.0.0:0" } else { "[::]:0"}.parse()?;

    let socket = UdpSocket::bind(local_addr).await?;
    socket.connect(remote_server).await?;

    let tun = create_tun(iface, address, 1500).await;

    let key:[u8;MAX_KEY_SIZE] = key_to_array(key).unwrap();
    let iv:[u8;MAX_IV_SIZE] = iv_to_array(iv).unwrap();

    let mut peers: HashMap<IpAddr, Arc<Peer>> = HashMap::new();
    let remote_tun_server: Ipv4Addr = gateway_ip.parse().unwrap();
    let insert_peer = Arc::new(Peer {
        socket_addr: remote_server,
        key,
        iv,
    });
    peers.insert(IpAddr::V4(remote_tun_server), insert_peer);

    let socket_arc = Arc::new(socket);
    let socket_clone = socket_arc.clone();

    let client = Node {
        socket: socket_clone,
        tun,
        key,
        iv,
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

    let key = &hex_to_bytes(&cli.key)[..];
    let iv = &hex_to_bytes(&cli.iv)[..];
    match cli.mode {
        Mode::Server => {
            let address: String = if cli.address.is_empty() { DEFAULT_SERVER_ADDRESS.to_owned() } else { cli.address };
            info!("Server dev({} - {}) in port {} started", cli.iface, address, cli.port);
            let _ = server_mode(cli.port, key, iv, &cli.iface, &address).await;
        }
        Mode::Client => {
            let address: String = if cli.address.is_empty() { DEFAULT_CLIENT_ADDRESS.to_owned() } else { cli.address };

            if cli.host.is_empty() {
                error!("host is required for client mode");
                std::process::exit(1);
            }
            if cli.gateway.is_empty() {
                error!("gateway is required for client mode");
                std::process::exit(2);
            }
            info!("Client dev({} - {}), connect to {}:{} gw {}", &cli.iface, &address, cli.host, cli.port, cli.gateway);
            let _ = client_mode(&cli.host, key, iv, cli.port, &cli.gateway, &cli.iface, &address).await;
        }
    }
}