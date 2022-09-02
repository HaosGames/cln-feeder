mod db;
mod rpc;

use crate::db::{create_table, query_last_channel_values, store_current_values};
use crate::rpc::{get_current_fees, get_revenue_since};
use anyhow::Result;
use chrono::{Duration, Utc};
use clap::Parser;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use env_logger::WriteStyle;
use log::{debug, info, LevelFilter};
use sqlx::{Connection, SqliteConnection};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to the CLN Socket. Usually in `./clightning/bitcoin/lightning-rpc`
    #[clap(short, long, value_parser, value_name = "SOCKET")]
    socket: PathBuf,

    /// Path to the data directory that feeder uses.
    #[clap(short, long, value_parser, value_name = "FILE", default_value_t = String::from("~/.config/cln-feeder/cln-feeder.sqlite"))]
    database: String,

    /// Use a temporary sqlite database stored in memory
    #[clap(short, long, action)]
    temp_database: bool,

    /// Log Level
    #[clap(short, long, action = clap::ArgAction::Count, default_value_t = 0)]
    verbose: u8,

    /// Log Filter
    #[clap(short, long, default_value_t = String::from("cln_feeder"))]
    log_filter: String,

    /// Fee adjustment
    #[clap(short, long, default_value_t = 20, value_name = "MSATS")]
    fee_adjustment: u32,

    /// Past epochs to take into account when calculating new fees
    #[clap(short, long, default_value_t = 1)]
    epochs: u32,

    /// The length of an epoch in seconds
    #[clap(short, long, default_value_t = 12000, value_name = "SECONDS")]
    epoch_length: u32,
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
    let _ = if cli.log_filter.as_str() != "" {
        env_logger::builder()
            .write_style(WriteStyle::Always)
            .format_timestamp(None)
            .filter_module(cli.log_filter.as_str(), level)
            .init();
    } else {
        env_logger::builder()
            .write_style(WriteStyle::Always)
            .format_timestamp(None)
            .filter_level(level)
            .init();
    };

    info!("Creating RPC connection to CLN on {:?}", cli.socket);
    let mut client = ClnRpc::new(cli.socket)
        .await
        .expect("Couldn't connect to RPC Socket");

    let sqlite_conn = if cli.temp_database {
        String::from("sqlite::memory:")
    } else {
        if tokio::fs::File::open(cli.database.clone()).await.is_err() {
            tokio::fs::File::create(cli.database.clone()).await.unwrap();
        }
        cli.database
    };

    info!("Connecting to database {}", sqlite_conn.clone());
    let mut db = SqliteConnection::connect(sqlite_conn.as_str()).await?;
    create_table(&mut db).await.expect("Couldn't create table");

    loop {
        debug!("New Iteration");
        iterate(cli.epochs, cli.epoch_length, cli.fee_adjustment, &mut client, &mut db).await?;
        tokio::time::sleep(std::time::Duration::from_secs(cli.epoch_length.into())).await;
    }
}
async fn iterate(epochs: u32, epoch_length: u32, fee_adjustment: u32, client: &mut ClnRpc, db: &mut SqliteConnection) -> Result<()> {
    let current_fees = get_current_fees(client).await;
    for (id, current_fee) in current_fees {
        let current_revenue =
            get_revenue_since(epoch_length, ShortChannelId::from_str(id.as_str()).unwrap(), client).await;
        if let Ok(last_values) = query_last_channel_values(&id, epochs, db).await {
            if let Some((last_updated, _, _)) = last_values.get(0) {
                if last_updated
                    > &(Utc::now() - Duration::seconds(epoch_length as i64)).timestamp()
                {
                    continue;
                }
            }
            let new_fee = new_fee(last_values, current_fee, current_revenue as u32, fee_adjustment).await;
            info!("New fee {} -> {} msats for {}", current_fee, new_fee, id);
            // TODO set new fee
        }
        store_current_values(db, &id, current_fee, current_revenue as u32).await?;
    }
    Ok(())
}
async fn new_fee(
    last_values: Vec<(i64, u32, u32)>,
    current_fee: u32,
    current_revenue: u32,
    fee_adjustment_msats: u32,
) -> u32 {
    let (last_fee, last_revenue) = if !last_values.is_empty() {
        let (mut average_fee, mut average_revenue) = (0u32, 0u32);
        for (_time, fee, revenue) in &last_values {
            average_fee += fee;
            average_revenue += revenue;
        }
        average_fee /= last_values.len() as u32;
        average_revenue /= last_values.len() as u32;
        (average_fee, average_revenue)
    } else {
        // Starting fee
        return 1000;
    };

    return if current_revenue > last_revenue {
        if current_fee > last_fee {
            current_fee + (current_fee - last_fee)
        } else if current_fee < last_fee {
            current_fee + (last_fee - current_fee) / 2
        } else {
            current_fee + fee_adjustment_msats
        }
    } else if current_revenue < last_revenue {
        if current_fee > last_fee {
            current_fee - (current_fee - last_fee) / 2
        } else if current_fee < last_fee {
            current_fee - (last_fee - current_fee)
        } else {
            current_fee - fee_adjustment_msats
        }
    } else {
        if current_fee > last_fee {
            current_fee
        } else if current_fee < last_fee {
            last_fee
        } else {
            current_fee + fee_adjustment_msats
        }
    };
}
