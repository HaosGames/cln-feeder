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
use log::{debug, info, trace, LevelFilter};
use rusqlite::Connection;
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

    /// A divisor by which the current fees are divided when an absolute value must be found to calculate the new fees.
    #[clap(short, long, default_value_t = 10, value_name = "UINT")]
    adjustment_divisor: u32,

    /// Past epochs to take into account when calculating new fees
    #[clap(short = 'e', long, default_value_t = 3)]
    epochs: u32,

    /// The length of an epoch in hours
    #[clap(short = 'E', long, default_value_t = 24, value_name = "HOURS")]
    epoch_length: u32,
}

#[allow(clippy::let_unit_value)]
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
        Connection::open_in_memory().expect("Couldn't open database in memory")
    } else {
        tokio::fs::create_dir_all(cli.data_dir)
            .await
            .expect("Couldn't create data dir");
        Connection::open(db_path).expect("Couldn't open database")
    };
    create_table(&mut db);
    assert!(
        cli.adjustment_divisor != 0,
        "The divisor must be bigger than 0"
    );

    loop {
        debug!("New Iteration");
        iterate(
            cli.epochs,
            cli.epoch_length,
            cli.adjustment_divisor,
            &mut client,
            &mut db,
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
    }
}
async fn iterate(
    epochs: u32,
    epoch_length: u32,
    adjustment_divisor: u32,
    client: &mut ClnRpc,
    db: &mut Connection,
) {
    let current_fees = get_current_fees(client).await;
    for (id, current_fee) in current_fees {
        let current_revenue = get_revenue_since(
            epoch_length,
            ShortChannelId::from_str(id.as_str()).unwrap(),
            client,
        )
        .await;
        trace!(
            "Current values for {}:[fee: {}, revenue: {}]",
            id,
            current_fee,
            current_revenue
        );
        let last_values = query_last_channel_values(&id, epochs, db);
        trace!("Queried last channel values for {}", id);
        if let Some((last_updated, _, _)) = last_values.first() {
            if last_updated > &(Utc::now() - Duration::hours(epoch_length.into())).timestamp() {
                trace!(
                    "Skipped iteration for {} because current epoch is still ongoing",
                    id
                );
                continue;
            }
        }
        let mut values: Vec<(u32, u32)> = last_values
            .iter()
            .map(|(_, fee, revenue)| (*fee, *revenue))
            .collect();
        values.insert(0, (current_fee, current_revenue.try_into().unwrap()));
        if let Some(new_fee) = NewFees::calculate(&values, adjustment_divisor) {
            info!("New fee {} -> {} msats for {}", current_fee, new_fee, id);
            set_channel_fee(client, &id, new_fee).await;
        }
        store_current_values(db, id, current_fee, current_revenue as u32);
    }
}
#[derive(Default, Clone)]
#[allow(unused)]
struct NewFees {
    average_fee: i64,
    average_revenue: i64,
    present_fee: i64,
    present_revenue: i64,
    past_fee: i64,
    past_revenue: i64,
    current_fee: i64,
    current_revenue: i64,
    adjustment_fee: i64,
}
#[allow(unused)]
impl NewFees {
    pub fn calculate(values: &Vec<(u32, u32)>, adjustment_divisor: u32) -> Option<u32> {
        if values.len() < 2 {
            trace!("No last values -> No new fee");
            return None;
        }
        let mut p = Self::default();
        let (mut first_n, mut last_n) = (0, 0);
        for (i, (fee, revenue)) in values.iter().enumerate() {
            if i <= (values.len() - 1) / 3 {
                p.present_fee += (*fee) as i64;
                p.present_revenue += (*revenue) as i64;
                first_n += 1;
            }
            if i >= 2 * values.len() / 3 {
                p.past_fee += (*fee) as i64;
                p.past_revenue += (*revenue) as i64;
                last_n += 1;
            }
            p.average_fee += (*fee) as i64;
            p.average_revenue += (*revenue) as i64;
        }
        p.present_fee /= first_n;
        p.present_revenue /= first_n;
        p.past_fee /= last_n;
        p.past_revenue /= last_n;
        p.average_fee /= values.len() as i64;
        p.average_revenue /= values.len() as i64;

        let (current_fee, current_revenue) = *values.first().unwrap();
        p.current_fee = current_fee.into();
        p.current_revenue = current_revenue.into();

        p.adjustment_fee = if current_fee / adjustment_divisor != 0 {
            current_fee / adjustment_divisor
        } else {
            1
        } as i64;
        p.determine()
    }
    #[allow(clippy::if_same_then_else)]
    fn determine(&self) -> Option<u32> {
        let new_fee: i64 = if self.average_revenue == 0 {
            self.current_fee / 2
        } else if self.rev_is_rising() {
            if self.fee_is_rising() {
                self.step_up()
            } else if self.fee_is_falling() {
                self.step_down()
            } else if self.fee_has_higher_average() {
                self.adjust_up()
            } else if self.fee_has_lower_average() {
                self.adjust_up()
            } else {
                return None;
            }
        } else if self.rev_is_falling() {
            if self.fee_is_falling() {
                self.run_down()
            } else if self.fee_is_rising() {
                self.adjust_down()
            } else if self.fee_has_higher_average() {
                self.step_down()
            } else if self.fee_has_lower_average() {
                self.adjust_down()
            } else {
                return None;
            }
        } else if self.rev_has_higher_average() {
            if self.fee_is_rising() {
                self.adjust_down()
            } else if self.fee_is_falling() {
                self.adjust_up()
            } else if self.fee_has_higher_average() {
                self.adjust_up()
            } else if self.fee_has_lower_average() {
                self.adjust_down()
            } else {
                return None;
            }
        } else if self.rev_has_lower_average() {
            if self.fee_is_rising() {
                return None;
            } else if self.fee_is_falling() {
                self.adjust_up()
            } else if self.fee_has_higher_average() {
                self.step_down()
            } else if self.fee_has_lower_average() {
                self.step_up()
            } else {
                return None;
            }
        } else {
            return None;
        };

        if new_fee <= 0 {
            return Some(1);
        }
        Some(new_fee.try_into().unwrap())
    }
    fn fee_should_step_up(&self) -> bool {
        self.rev_is_rising()
    }
    fn fee_should_step_down(&self) -> bool {
        self.rev_is_falling()
    }
    fn fee_should_adjust(&self) -> bool {
        self.rev_has_lower_average() || self.rev_has_higher_average()
    }
    fn rev_is_rising(&self) -> bool {
        self.past_revenue < self.average_revenue && self.average_revenue < self.present_revenue
    }
    fn rev_is_falling(&self) -> bool {
        self.past_revenue > self.average_revenue && self.average_revenue > self.present_revenue
    }
    fn rev_has_lower_average(&self) -> bool {
        self.past_revenue >= self.average_revenue && self.average_revenue < self.present_revenue
    }
    fn rev_has_higher_average(&self) -> bool {
        self.past_revenue <= self.average_revenue && self.average_revenue > self.present_revenue
    }
    fn fee_is_rising(&self) -> bool {
        self.past_fee < self.average_fee && self.average_fee < self.present_fee
    }
    fn fee_is_falling(&self) -> bool {
        self.past_fee > self.average_fee && self.average_fee > self.present_fee
    }
    fn fee_has_lower_average(&self) -> bool {
        self.past_fee >= self.average_fee && self.average_fee < self.present_fee
    }
    fn fee_has_higher_average(&self) -> bool {
        self.past_fee <= self.average_fee && self.average_fee > self.present_fee
    }
    fn fee_steps_up(&self) -> bool {
        self.average_fee < self.present_fee && self.present_fee < self.current_fee
    }
    fn fee_steps_down(&self) -> bool {
        self.average_fee > self.present_fee && self.present_fee > self.current_fee
    }
    fn fee_adjusts(&self) -> bool {
        (self.average_fee > self.present_fee && self.present_fee < self.current_fee)
            || (self.average_fee < self.present_fee && self.present_fee > self.current_fee)
    }
    fn step_up(&self) -> i64 {
        self.current_fee + self.adjustment_fee
    }
    fn step_down(&self) -> i64 {
        self.current_fee - self.adjustment_fee
    }
    fn adjust_up(&self) -> i64 {
        self.current_fee + (self.present_fee - self.current_fee).abs() / 2
    }
    fn adjust_down(&self) -> i64 {
        self.current_fee - (self.present_fee - self.current_fee).abs() / 2
    }
    fn run_up(&self) -> i64 {
        self.current_fee + (self.current_fee - self.present_fee).abs() * 2
    }
    fn run_down(&self) -> i64 {
        self.current_fee - (self.current_fee - self.present_fee).abs() * 2
    }
}
#[allow(unused)]
async fn new_fee(
    last_values: Vec<(u32, u32)>,
    current_fee: u32,
    current_revenue: u32,
    adjustment_divisor: u32,
) -> Option<u32> {
    let (current_fee, current_revenue, adjustment_divisor): (i64, i64, i64) = (
        current_fee.into(),
        current_revenue.into(),
        adjustment_divisor.into(),
    );
    let (last_fee, last_revenue) = if !last_values.is_empty() {
        let (mut average_fee, mut average_revenue) = (0, 0);
        for (fee, revenue) in &last_values {
            average_fee += fee;
            average_revenue += revenue;
        }
        average_fee /= last_values.len() as u32;
        average_revenue /= last_values.len() as u32;
        let (average_fee, average_revenue): (i64, i64) =
            (average_fee.into(), average_revenue.into());
        if last_values.len() > 1 {
            trace!(
                "Last average values: [fee: {}, revenue: {}]",
                average_fee,
                average_revenue
            );
        } else {
            trace!(
                "Last values: [fee: {}, revenue: {}]",
                average_fee,
                average_revenue
            );
        }
        (average_fee, average_revenue)
    } else {
        trace!("No last values -> No new fee");
        return None;
    };
    let fee_adjustment = if current_fee / adjustment_divisor != 0 {
        current_fee / adjustment_divisor
    } else {
        1
    };

    use std::cmp::Ordering;
    let new_fee = match current_revenue.cmp(&last_revenue) {
        Ordering::Less => match current_fee.cmp(&last_fee) {
            Ordering::Less => current_fee - (last_fee - current_fee) * 2,
            Ordering::Equal => current_fee - fee_adjustment,
            Ordering::Greater => current_fee - (current_fee - last_fee) / 2,
        },
        Ordering::Equal => {
            if current_revenue == 0 {
                match current_fee.cmp(&last_fee) {
                    Ordering::Less => current_fee / 2,
                    Ordering::Equal => current_fee - fee_adjustment,
                    Ordering::Greater => last_fee - fee_adjustment,
                }
            } else {
                match current_fee.cmp(&last_fee) {
                    Ordering::Less => last_fee,
                    Ordering::Equal => current_fee,
                    Ordering::Greater => current_fee,
                }
            }
        }
        Ordering::Greater => match current_fee.cmp(&last_fee) {
            Ordering::Less => current_fee + (last_fee - current_fee) / 2,
            Ordering::Equal => current_fee + fee_adjustment,
            Ordering::Greater => current_fee + (current_fee - last_fee) * 2,
        },
    };
    if new_fee <= 0 {
        return Some(1);
    }
    Some(new_fee.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[ignore]
    #[tokio::test]
    async fn decrease_when_zero_revenue() {
        let mut values = vec![(500, 0)];
        let fee = new_fee(values.clone(), 500, 0, 10).await.unwrap();
        assert_eq!(fee, 450);
        values.push((500, 0));
        let fee = new_fee(values.clone(), fee, 0, 10).await.unwrap();
        assert_eq!(fee, 225);
        values.push((450, 0));
        let fee = new_fee(values.clone(), fee, 100, 10).await.unwrap();
        assert_eq!(fee, 354);
        values.push((225, 0));
        let fee = new_fee(values.clone(), fee, 80, 10).await.unwrap();
        assert_eq!(fee, 354);
    }
}
