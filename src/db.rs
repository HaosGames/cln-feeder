use anyhow::Result;
use chrono::Utc;
use cln_rpc::primitives::ShortChannelId;
use sqlx::{Executor, Row, SqliteConnection};
use std::collections::HashMap;
use std::str::FromStr;

pub async fn store_current_values(
    db: &mut SqliteConnection,
    id: String,
    fee: u32,
    revenue: u32,
) -> Result<()> {
    db.execute(
        format!(
            "INSERT OR REPLACE INTO channels (short_channel_id, last_fee, last_revenue, last_updated)\
                     SET {}, {}, {}, {}",
            id,
            fee,
            revenue,
            Utc::now().timestamp(),
        )
            .as_str(),
    )
        .await?;
    Ok(())
}
pub async fn create_table(db: &mut SqliteConnection) -> Result<()> {
    db.execute(
        "CREATE TABLE IF NOT EXIST channels \
    (short_channel_id PRIMARY KEY, last_fee, last_revenue, last_updated PRIMARY KEY)",
    )
    .await?;
    Ok(())
}
pub async fn query_last_values(
    db: &mut SqliteConnection,
) -> Result<HashMap<String, (u32, u32, i64)>> {
    let values = db
        .fetch_all("SELECT short_channel_id, last_fee, last_revenue, last_updated FROM channels")
        .await?;
    let mut result = HashMap::new();
    for row in values {
        let id = ShortChannelId::from_str(row.get("short_channel_id"))
            .unwrap()
            .to_string();
        let last_fee: u32 = row.get("last_fee");
        let last_revenue: u32 = row.get("last_revenue");
        let last_updated: i64 = row.get("last_updated");
        result.insert(id, (last_fee, last_revenue, last_updated));
    }
    Ok(result)
}
