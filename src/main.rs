use clap::{Parser, ValueEnum};
use core::str;
use env_logger::Builder;
use ipnet::Ipv4Net;
use log::{debug, error, info, LevelFilter};
use std::error::Error;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use crate::codificar::*;
pub mod codificar;
use crate::route::*;
pub mod route;
use crate::device_tun::*;
pub mod device_tun;
use crate::node::*;
pub mod node;

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
