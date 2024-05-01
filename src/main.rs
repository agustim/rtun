use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(version = "0.0.1", about, long_about = None)]

struct Cli {
    #[arg(value_enum, short, long)]
    mode: Mode,
    #[arg(short, long, default_value = "1714")]
    port: u16,
    #[arg(short = 'o', long, default_value = "")]
    host: String,
}
#[derive(Clone, ValueEnum)]
enum Mode {
    Server,
    Client,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.mode {
        Mode::Server => {
            println!("server in port {} started", cli.port);
        }
        Mode::Client => {
            if cli.host.is_empty() {
                eprintln!("host is required for client mode");
                std::process::exit(1);
            }
            println!("client, connect to {}:{}", cli.host, cli.port);
        }
    }
}
