use clap::{Parser, ValueEnum};
use std::net::{SocketAddr, IpAddr};
use std::str::FromStr;
use std::error::Error;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::timeout;
use udp_stream::UdpStream;

const UDP_BUFFER_SIZE: usize = 17480; // 17kb
const UDP_TIMEOUT: u64 = 10 * 1000; // 10sec

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

async fn server_mode(port: u16) -> Result<(), Box<dyn Error>> {

    let localhost_ip = IpAddr::from_str("127.0.0.1").unwrap();
    let server = SocketAddr::new(localhost_ip, port);
    let listener = udp_stream::UdpListener::bind(server).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        println!("Connected to {}", &stream.peer_addr()?);
        tokio::spawn(async move {
            let id = std::thread::current().id();
            let block = async move {
                let mut buf = vec![0u8; UDP_BUFFER_SIZE];
                let duration = Duration::from_millis(UDP_TIMEOUT);
                loop {
                    let n = timeout(duration, stream.read(&mut buf)).await??;
                    stream.write_all(&buf[0..n]).await?;
                    println!("{:?} echoed {:?} for {} bytes", id, stream.peer_addr(), n);
                }
                #[allow(unreachable_code)]
                Ok::<(), std::io::Error>(())
            };
            if let Err(e) = block.await {
                println!("error: {:?}", e);
            }
        });
        println!("out of spawn");
    }

}

async fn client_mode(server_ip: &str, port: u16) -> Result<(), Box<dyn Error>> {

    let ip_server = IpAddr::from_str(server_ip).unwrap();
    let server = SocketAddr::new(ip_server, port);
    let mut stream = UdpStream::connect(server).await?;

    println!("Ready to Connected to {}", &stream.peer_addr()?);
    let mut buffer = String::new();
    loop {
        std::io::stdin().read_line(&mut buffer).unwrap();
        stream.write_all(buffer.as_bytes()).await?;
        let mut buf = vec![0u8; 1024];
        let n = stream.read(&mut buf).await?;
        print!("-> {}", String::from_utf8_lossy(&buf[..n]));
        buffer.clear();
    }
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
