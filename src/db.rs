use anyhow::Result;
use chrono::Utc;
use log::debug;
use sqlx::{query, Executor, Row, SqliteConnection};

pub async fn store_current_values(
    db: &mut SqliteConnection,
    id: &String,
    fee: u32,
    revenue: u32,
) -> Result<()> {
    let now = Utc::now().timestamp();
    db.execute(query!(
        "INSERT OR REPLACE INTO channels (short_channel_id, last_fee, last_revenue, last_updated)\
                     VALUES (?, ?, ?, ?)",
        id,
        fee,
        revenue,
        now,
    ))
    .await?;
    debug!("Stored [fee: {} msats, revenue: {} msats] for {}", fee, revenue, id);
    Ok(())
}
pub async fn create_table(db: &mut SqliteConnection) -> Result<()> {
    db.execute( query!(
        "CREATE TABLE IF NOT EXISTS channels \
    (short_channel_id NON NULL, last_fee NON NULL, last_revenue NON NULL, last_updated NON NULL, PRIMARY KEY (short_channel_id, last_updated))",
    ))
    .await?;
    Ok(())
}
pub async fn query_last_channel_values(
    short_channel_id: &String,
    count: u32,
    db: &mut SqliteConnection,
) -> Result<Vec<(i64, u32, u32)>> {
    let values = db
        .fetch_all(format!(
            "SELECT short_channel_id, last_fee, last_revenue, last_updated FROM channels WHERE short_channel_id IS {} ORDER BY last_updated DESC LIMIT {}",
            short_channel_id,
            count
        ).as_str())
        .await?
        .iter()
        .map(|row|{
            (
                row.get("last_updated"),
                row.get("last_fee"),
                row.get("last_revenue")
            )
        })
        .collect();
    Ok(values)
}
