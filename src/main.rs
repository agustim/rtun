use clap::{Parser, ValueEnum};
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use tokio::net::UdpSocket;
use env_logger::Builder;
use log::{debug, error, info, LevelFilter};
use ipnet::Ipv4Net;
use std::net::Ipv4Addr;
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
    buf: Vec<u8>,
    peer: SocketAddr,  // Així no podem tenir més d'un client, s'hauria de canviar per un HashMap
    to_send_tun: Option<(usize, SocketAddr)>,
    recv_from_tun: Option<usize>
}

impl Node {
    async fn run(self) -> Result<(), Box<dyn Error>> {
        let Node {
            socket,
            tun,
            mut peer,
            mut buf,
            mut to_send_tun,
            mut recv_from_tun,
        } = self;

        loop {
            if let Some((size, peer_connect)) = to_send_tun {

                debug!("Received {}/{} bytes from {}", size, UDP_BUFFER_SIZE, peer_connect);
                peer = peer_connect;
                tun.send(&buf[..size]).await?;
            }
            
            if let Some(size) = recv_from_tun {
                debug!("Received {}/{} bytes ", size, UDP_BUFFER_SIZE);
                
                let _ = socket.send_to(&buf[..size], &peer).await?;

            }

            to_send_tun = Some(socket.recv_from(&mut buf).await?);

            recv_from_tun = Some(tun.recv(&mut buf).await.unwrap());

        }
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
        peer: SocketAddr::new(localhost_ip, 0),
        buf: vec![0; UDP_BUFFER_SIZE],
        to_send_tun: None,
        recv_from_tun: None,
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
        peer: remote_server,
        buf: vec![0; UDP_BUFFER_SIZE],
        to_send_tun: None,
        recv_from_tun: None,
    };
    client.run().await?;

    Ok(())
}

#[tokio::main]
async fn main() {
    Builder::new()
        .filter(None, LevelFilter::Info)
        .init();

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
