use clap::Parser;
use std::path::PathBuf;
use cln_rpc::{ClnRpc, Request};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(short, long, value_parser, value_name = "FILE")]
    socket: PathBuf,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let mut client = ClnRpc::new(cli.socket).await.unwrap();
    let getinfo = client.call(Request::Getinfo(cln_rpc::model::GetinfoRequest {})).await.unwrap();
    println!("{:?}", getinfo);
}
