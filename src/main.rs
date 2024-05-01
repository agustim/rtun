use clap::{Parser, ValueEnum};
use tokio::net::UdpSocket;
use std::net::{IpAddr, SocketAddr};
use std::error::Error;
use std::str::FromStr;
use std::io::{stdin, Read};


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
}
#[derive(Clone, ValueEnum)]
enum Mode {
    Server,
    Client,
}

struct Server {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl Server {
    async fn run(self) -> Result<(), Box<dyn Error>> {
        let Server {
            socket,
            mut buf,
            mut to_send,
        } = self;

        loop {
            if let Some((size, peer)) = to_send {
                let amt = socket.send_to(&buf[..size], &peer).await?;

                println!("Echoed {}/{} bytes to {}", amt, size, peer);
            }

            to_send = Some(socket.recv_from(&mut buf).await?);
        }
    }
}

async fn server_mode(port: u16) -> Result<(), Box<dyn Error>> {

    let localhost_ip = IpAddr::from_str("127.0.0.1").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let socket = UdpSocket::bind(server).await?;

    let server = Server {
        socket,
        buf: vec![0; UDP_BUFFER_SIZE],
        to_send: None,
    };
        server.run().await?;

    Ok(())
    
}
fn get_stdin_data() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut buf = Vec::new();
    stdin().read_to_end(&mut buf)?;
    Ok(buf)
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
    const MAX_DATAGRAM_SIZE: usize = 65_507;
    socket.connect(remote_server).await?;
    let data = get_stdin_data()?;
    println!("Sending data:\n{}", String::from_utf8_lossy(&data));
    socket.send(&data).await?;
    let mut data = vec![0u8; MAX_DATAGRAM_SIZE];
    let len = socket.recv(&mut data).await?;
    println!("Received {} bytes:\n{}",len,String::from_utf8_lossy(&data[..len]));

    Ok(())

}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.mode {
        Mode::Server => {
            println!("server in port {} started", cli.port);
            let _ = server_mode(cli.port).await;
        }
        Mode::Client => {
            if cli.host.is_empty() {
                eprintln!("host is required for client mode");
                std::process::exit(1);
            }
            println!("client, connect to {}:{}", cli.host, cli.port);
            let _ = client_mode(&cli.host, cli.port).await;
        }
    }
}
