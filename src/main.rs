use clap::{Parser, ValueEnum};


#[derive(Parser)]
#[command(version = "0.0.1", about, long_about = None)]

struct Cli {
    #[arg(value_enum, short, long)]
    mode: Mode,
    #[arg(short, long, default_value = "1714")]
    port: u16,

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
            println!("server");
        }
        Mode::Client => {
            println!("client");
        }
    }

}
