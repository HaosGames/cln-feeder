use clap::Parser;
use std::path::PathBuf;
use cln_rpc::{ClnRpc, Request, Response};
use env_logger::WriteStyle;
use log::LevelFilter;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to the CLN Socket. Usually in `./clightning/bitcoin/lightning-rpc`
    #[clap(short, long, value_parser, value_name = "FILE")]
    socket: PathBuf,

    /// Log Level
    #[clap(short, long, action = clap::ArgAction::Count, default_value_t = 3)]
    verbose: u8,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let level = match cli.verbose {
        4 => LevelFilter::Trace,
        3 => LevelFilter::Debug,
        2 => LevelFilter::Info,
        1 => LevelFilter::Warn,
        _ => LevelFilter::Error,
    };
    let _ = env_logger::builder()
    .write_style(WriteStyle::Always)
    .format_timestamp(None)
    .filter_level(level)
    //.filter_module("cln-feeder", LevelFilter::Trace)
    .init();

    let mut client = ClnRpc::new(cli.socket).await.unwrap();
    print_channel_fees(&mut client).await;
}
async fn print_channel_fees(client: &mut ClnRpc) {
    let response = client.call(Request::ListPeers(cln_rpc::model::ListpeersRequest { id: None, level: None })).await.unwrap();
    if let Response::ListPeers(response) = response {
        for peer in response.peers {
            for channel in peer.channels {
                println!(
                    "{:?}: My fee: {}",
                    channel.short_channel_id.unwrap(),
                    channel.fee_proportional_millionths.unwrap()
                )
            }
        }
    }
}
