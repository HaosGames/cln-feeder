mod db;
mod rpc;

use crate::db::{create_table, query_last_channel_values, store_current_values};
use crate::rpc::{get_current_fees, get_revenue_since, set_channel_fee};
use anyhow::Result;
use chrono::{Duration, Utc};
use clap::Parser;
use cln_rpc::primitives::ShortChannelId;
use cln_rpc::ClnRpc;
use env_logger::WriteStyle;
use log::{debug, error, info, LevelFilter};
use sqlx::{Connection, SqliteConnection};
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Path to the CLN Socket. Usually in `./clightning/bitcoin/lightning-rpc`
    #[clap(short, long, value_parser, value_name = "PATH")]
    socket: PathBuf,

    /// Path to the data directory that feeder uses
    #[clap(
        short,
        long,
        value_parser,
        value_name = "PATH",
        default_value = "~/.local/cln-feeder/"
    )]
    data_dir: PathBuf,

    /// Use a temporary sqlite database stored in memory
    #[clap(short, long, action)]
    temp_database: bool,

    /// Log Level
    #[clap(short, long, action = clap::ArgAction::Count, default_value_t = 0)]
    verbose: u8,

    /// Log Filter
    #[clap(short, long, default_value_t = String::from("cln_feeder"), value_name = "STRING")]
    log_filter: String,

    /// Fee adjustment
    #[clap(short, long, default_value_t = 20, value_name = "MSATS")]
    fee_adjustment: u32,

    /// Past epochs to take into account when calculating new fees
    #[clap(short = 'e', long, default_value_t = 7)]
    epochs: u32,

    /// The length of an epoch in hours
    #[clap(short = 'E', long, default_value_t = 12, value_name = "HOURS")]
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

    let db_path = cli.data_dir.join("./feeder.sqlite");

    info!("Connecting to database {:?}", db_path);
    let mut db = if cli.temp_database {
        SqliteConnection::connect("sqlite::memory:").await?
    } else {
        tokio::fs::create_dir_all(cli.data_dir)
            .await
            .expect("Couldn't create data dir");
        if tokio::fs::File::open(db_path.clone()).await.is_err() {
            tokio::fs::File::create(db_path.clone())
                .await
                .expect("Couldn't create database file");
        }
        SqliteConnection::connect(db_path.to_str().unwrap()).await?
    };
    create_table(&mut db).await.expect("Couldn't create table");

    loop {
        debug!("New Iteration");
        iterate(
            cli.epochs,
            cli.epoch_length,
            cli.fee_adjustment,
            &mut client,
            &mut db,
        )
        .await?;
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
}
async fn iterate(
    epochs: u32,
    epoch_length: u32,
    fee_adjustment: u32,
    client: &mut ClnRpc,
    db: &mut SqliteConnection,
) -> Result<()> {
    let current_fees = get_current_fees(client).await;
    for (id, current_fee) in current_fees {
        let current_revenue = get_revenue_since(
            epoch_length,
            ShortChannelId::from_str(id.as_str()).unwrap(),
            client,
        )
        .await;
        match query_last_channel_values(&id, epochs, db).await {
            Ok(last_values) => {
                if let Some((last_updated, _, _)) = last_values.first() {
                    if last_updated > &(Utc::now() - Duration::hours(epoch_length.into())).timestamp() {
                        debug!("Skipped iteration for {} because current epoch is still ongoing", id);
                        continue;
                    }
                }
                let new_fee = new_fee(
                    last_values,
                    current_fee,
                    current_revenue as u32,
                    fee_adjustment,
                )
                    .await;
                info!("New fee {} -> {} msats for {}", current_fee, new_fee, id);
                set_channel_fee(client, &id, new_fee).await;
            }
            Err(e) => error!("Couldn't query last values for {}: {}", id, e),
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
        return 500;
    };

    use std::cmp::Ordering;
    match current_revenue.cmp(&last_revenue) {
        Ordering::Less => match current_fee.cmp(&last_fee) {
            Ordering::Less => current_fee - (last_fee - current_fee) * 2,
            Ordering::Equal => current_fee - fee_adjustment_msats,
            Ordering::Greater => current_fee - (current_fee - last_fee) / 2,
        },
        Ordering::Equal => match current_fee.cmp(&last_fee) {
            Ordering::Less => last_fee,
            Ordering::Equal => current_fee + fee_adjustment_msats,
            Ordering::Greater => current_fee,
        },
        Ordering::Greater => match current_fee.cmp(&last_fee) {
            Ordering::Less => current_fee + (last_fee - current_fee) / 2,
            Ordering::Equal => current_fee + fee_adjustment_msats,
            Ordering::Greater => current_fee + (current_fee - last_fee) * 2,
        },
    }
}
