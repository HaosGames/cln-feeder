use anyhow::Result;
use chrono::Utc;
use log::debug;
use rusqlite::Connection;

pub fn store_current_values(db: &mut Connection, id: String, fee: u32, revenue: u32) -> Result<()> {
    let now = Utc::now().timestamp();
    db.execute(
        "INSERT OR REPLACE INTO channels (short_channel_id, last_fee, last_revenue, last_updated)\
                     VALUES (?1, ?2, ?3, ?4)",
        (id.clone(), fee.clone(), revenue.clone(), now),
    )?;
    debug!(
        "Stored [fee: {} msats, revenue: {} msats] for {}",
        fee, revenue, id
    );
    Ok(())
}
pub fn create_table(db: &mut Connection) -> Result<()> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS channels \
    (short_channel_id NON NULL, \
    last_fee NON NULL, \
    last_revenue NON NULL, \
    last_updated NON NULL, \
    PRIMARY KEY (short_channel_id, last_updated))",
    ())
    ?;
    Ok(())
}
pub fn query_last_channel_values(
    short_channel_id: &String,
    count: u32,
    db: &mut Connection,
) -> Result<Vec<(i64, u32, u32)>> {
    let values = db
        .prepare(
            "SELECT short_channel_id, last_fee, last_revenue, last_updated FROM channels \
            WHERE short_channel_id IS ?1 ORDER BY last_updated DESC LIMIT ?2",
        )?
        .query([short_channel_id, &count.to_string()])?
        .mapped(|row|{
            Ok((
                row.get("last_updated").unwrap(),
                row.get("last_fee").unwrap(),
                row.get("last_revenue").unwrap()
            ))
        })
        .map(|row| {
            row.unwrap()
        })
        .collect();
    Ok(values)
}
