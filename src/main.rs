use clap::{arg, Command};

// const PORT: &str = "1714";

fn cli() -> Command {
    Command::new("RTun")
        .version("0.0.1")
        .author("Agustí Moll")
        .about("Building a simple VPN")
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .arg(arg!(-m <MODE> "Mode of the program")
            .required(true)
            .value_parser(["client", "server"])
        )
        // .subcommand(
        //     Command::new("mode")
        //         .about("Define the mode of the program")
        //         .arg(arg!(<MODE> "Mode of the program")
        //             .required(true)
        //             .value_parser(["client", "server"])
        //         )
        //         .arg_required_else_help(true)
        // )
    }
#[tokio::main]
async fn main() {

    let matches = cli().get_matches();

    match matches.contains_id(MODE) {
        Some("client") => {
            println!("Client mode");
        }
        Some("server") => {
            println!("Server mode");
        }
        _ => {
            println!("No mode selected");
        }
    }


}
