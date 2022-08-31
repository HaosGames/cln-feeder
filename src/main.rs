mod db;
mod rpc;

use crate::db::{create_table, query_last_values, store_current_values};
use crate::rpc::{get_current_fees, get_current_revenue};
use anyhow::Result;
use clap::Parser;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use env_logger::WriteStyle;
use log::{debug, info, LevelFilter};
use sqlx::{Connection, SqliteConnection};
use std::path::PathBuf;
use std::str::FromStr;

const UPDATE_INTERVAL_SECONDS: u32 = 1200;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to the CLN Socket. Usually in `./clightning/bitcoin/lightning-rpc`
    #[clap(short, long, value_parser, value_name = "SOCKET")]
    socket: PathBuf,

    /// Path to the data directory that feeder uses.
    #[clap(short, long, value_parser, value_name = "DIRECTORY", default_value_t = String::from("~/.config/cln-feeder"))]
    data_dir: String,

    /// Use a temporary sqlite database stored in memory
    #[clap(short, long, action)]
    temp_database: bool,

    /// Log Level
    #[clap(short, long, action = clap::ArgAction::Count, default_value_t = 0)]
    verbose: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
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

    info!("Creating RPC connection to CLN on {:?}", cli.socket);
    let mut client = ClnRpc::new(cli.socket).await.unwrap();


    tokio::fs::create_dir_all(cli.data_dir.clone()).await.expect("Couldn't create data dir");
    let sqlite_conn = if cli.temp_database {
        String::from("sqlite::memory:")
    } else {
        let path = cli.data_dir + "./feeder.sqlite";
        if tokio::fs::File::open(path.clone()).await.is_err() {
            tokio::fs::File::create(path.clone()).await.unwrap();
        }
        path

    };

    info!("Connecting to database {}", sqlite_conn.clone());
    let mut db = SqliteConnection::connect(sqlite_conn.as_str()).await?;
    create_table(&mut db).await.expect("Couldn't create table");

    loop {
        debug!("New Iteration");
        iterate(&mut client, &mut db).await?;
        tokio::time::sleep(std::time::Duration::from_secs(
            UPDATE_INTERVAL_SECONDS.into(),
        ))
        .await;
    }
}
async fn iterate(client: &mut ClnRpc, db: &mut SqliteConnection) -> Result<()> {
    let last = query_last_values(db).await?;
    let current_fees = get_current_fees(client).await;
    for (id, current_fee) in current_fees {
        let current_revenue =
            get_current_revenue(ShortChannelId::from_str(id.as_str()).unwrap(), client).await;
        if let Some((last_fee, last_revenue, _last_updated)) = last.get(id.as_str()) {
            let new_fee = new_fee(
                *last_fee,
                *last_revenue,
                current_fee,
                current_revenue as u32,
            )
                .await;
            info!("New fee {} msats for {}", new_fee, id);
            // TODO set new fee
        }
        store_current_values(db, id.clone(), current_fee, current_revenue as u32).await?;
    }
    Ok(())
}
async fn new_fee(last_revenue: u32, last_fee: u32, current_revenue: u32, current_fee: u32) -> u32 {
    return if current_revenue > last_revenue {
        if current_fee > last_fee {
            current_fee + (current_fee - last_fee)
        } else if current_fee < last_fee {
            current_fee + (last_fee - current_fee) / 2
        } else {
            current_fee + 10
        }
    } else if current_revenue < last_revenue {
        if current_fee > last_fee {
            current_fee - (current_fee - last_fee) / 2
        } else if current_fee < last_fee {
            current_fee - (last_fee - current_fee)
        } else {
            current_fee - 10
        }
    } else {
        if current_fee > last_fee {
            current_fee
        } else if current_fee < last_fee {
            last_fee
        } else {
            current_fee + 100
        }
    };
}
