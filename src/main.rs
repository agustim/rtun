use clap::{Parser, ValueEnum};
use env_logger::Builder;
use ipnet::Ipv4Net;
use log::{debug, error, info, LevelFilter};
use std::error::Error;
use std::net::Ipv4Addr;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio_tun::Tun;
use tokio::signal;

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
    #[arg(short, long, default_value = "10.9.0.1/24")]
    address: String,
}
#[derive(Clone, ValueEnum)]
enum Mode {
    Server,
    Client,
}

async fn create_tun(ipv4: &str, mtu: i32) -> tokio_tun::Tun {
    let net: Ipv4Net = ipv4.parse().unwrap();

    let tun = Tun::builder()
        .name("")
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
    socket: UdpSocket,
    tun: tokio_tun::Tun,
    peer: SocketAddr, // Així no podem tenir més d'un client, s'hauria de canviar per un HashMap
}

async fn receive_from_tun_and_send_to_socket(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
    peer: Arc<SocketAddr>,
) -> Result<(), Box<dyn Error + Send + Sync>> {


    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        let size = tun.recv(&mut buf).await?;
        debug!("t2s: Received {}/{} bytes from tun sent to: {}", size, UDP_BUFFER_SIZE, peer);
        let _ = socket.send(&buf[..size]).await?;
    }

}

async fn receive_from_socket_and_send_to_tun(
    socket: Arc<UdpSocket>,
    tun: Arc<tokio_tun::Tun>,
) -> Result<(), Box<dyn Error + Send + Sync>> {


    let mut buf = [0u8; UDP_BUFFER_SIZE];
    loop {
        let (size, peer) = socket.recv_from(&mut buf).await?;
        debug!("s2t: Received {}/{} bytes from {}", size, UDP_BUFFER_SIZE, peer);
        tun.send(&buf[..size]).await?;
    }

}

impl Node {
    async fn run(self) -> Result<(), Box<dyn Error>> {
        let Node {
            socket,
            tun,
            peer,
        } = self;

        debug!("Node started");

        // Clone udp socket
        let socket_arc = Arc::new(socket);
        let socket_clone = socket_arc.clone();
        // Clone tun
        let tun_arc = Arc::new(tun);
        let tun_clone = tun_arc.clone();
        // Clone peer
        let peer_arc = Arc::new(peer);
        

        tokio::select!{ 

            res = tokio::spawn(async move {
                // receive from tun and send to socket
                let _ = receive_from_tun_and_send_to_socket(socket_arc, tun_arc, peer_arc ).await;
            }) => { res.map_err(|e| e.into()) },

            res = tokio::spawn(async move {
                // receive from socket and send to tun
                let _ = receive_from_socket_and_send_to_tun(socket_clone, tun_clone).await;
            }) => { res.map_err(|e| e.into()) },

            res = signal::ctrl_c() => {
                res.map_err(|e| e.into())
            }
        }

            
        // tokio::spawn(async move {
        //     let mut buf = [0u8; UDP_BUFFER_SIZE];
        //     debug!("in spawn tun");
        //     debug!("Before Peer: {:?}", peer);
        //     loop {

        //         if let Some(size) = recv_from_tun {
        //             debug!("Received {}/{} bytes {:?}", size, UDP_BUFFER_SIZE, buf[..size].to_vec() );

        //             debug!("Peer: {:?}", peer);
        //             if peer.ip().is_unspecified() || peer.port() == 0 {
        //                 debug!("Peer not set, skipping packet");
        //             } else {
        //                 let _ = socket_clone.send_to(&buf[..size], &peer).await.unwrap();
        //             }
        //         }
        //         recv_from_tun = Some(tun_clone.recv(&mut buf).await.unwrap());
        //     }
        // });

        // let mut buf = [0u8; UDP_BUFFER_SIZE];
        // loop {
        //     debug!("Looping");

        //     if let Some((size, peer_connect)) = to_send_tun {
        //         debug!(
        //             "Received {}/{} bytes from {}",
        //             size, UDP_BUFFER_SIZE, peer_connect
        //         );
        //         //peer = peer_connect;
        //         tun_arc.send(&buf[..size]).await.unwrap();
        //     }
        //     to_send_tun = Some(socket.recv_from(&mut buf).await.unwrap());
        // }
    }
}

async fn server_mode(port: u16) -> Result<(), Box<dyn Error>> {
    let localhost_ip = IpAddr::from_str("127.0.0.1").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let socket = UdpSocket::bind(server).await?;
    let tun = create_tun("10.9.0.1/24", 1500).await;

    let server = Node {
        socket,
        tun,
        peer: SocketAddr::new(localhost_ip, 0)
    };
    server.run().await?;

    Ok(())
}

async fn client_mode(server_ip: &str, port: u16) -> Result<(), Box<dyn Error>> {
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

    let tun = create_tun("10.9.0.2/24", 1500).await;

    let client = Node {
        socket,
        tun,
        peer: remote_server
    };
    client.run().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    Builder::new().filter(None, LevelFilter::Debug).init();

    let cli = Cli::parse();

    match cli.mode {
        Mode::Server => {
            info!("server in port {} started", cli.port);
            let _ = server_mode(cli.port).await;
        }
        Mode::Client => {
            if cli.host.is_empty() {
                error!("host is required for client mode");
                std::process::exit(1);
            }
            info!("client, connect to {}:{}", cli.host, cli.port);
            let _ = client_mode(&cli.host, cli.port).await;
        }
    }
}
