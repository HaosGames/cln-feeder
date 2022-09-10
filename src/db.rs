use chrono::Utc;
use log::{debug, trace};
use rusqlite::Connection;

pub fn store_current_values(db: &mut Connection, id: String, fee: u32, revenue: u32) {
    let now = Utc::now().timestamp();
    db.execute(
        "INSERT OR REPLACE INTO channels (short_channel_id, last_fee, last_revenue, last_updated)\
                     VALUES (?1, ?2, ?3, ?4)",
        (id.clone(), fee, revenue, now),
    )
    .expect("Couldn't store current values");
    debug!(
        "{}: Stored [fee: {} msats, revenue: {} msats, time: {}]",
        id, fee, revenue, now
    );
}
pub fn create_table(db: &mut Connection) {
    db.execute(
        "CREATE TABLE IF NOT EXISTS channels \
    (short_channel_id NON NULL, \
    last_fee NON NULL, \
    last_revenue NON NULL, \
    last_updated NON NULL, \
    PRIMARY KEY (short_channel_id, last_updated))",
        (),
    )
    .expect("Couldn't create database table");
    trace!("Created database table");
}
pub fn query_last_channel_values(
    short_channel_id: &String,
    count: u32,
    db: &mut Connection,
) -> Vec<(i64, u32, u32)> {
    db.prepare(
        "SELECT short_channel_id, last_fee, last_revenue, last_updated FROM channels \
            WHERE short_channel_id IS ?1 ORDER BY last_updated DESC LIMIT ?2",
    )
    .expect("Preparing query for last values failed")
    .query([short_channel_id, &count.to_string()])
    .expect("Couldn't bind parameters to query")
    .mapped(|row| {
        Ok((
            row.get("last_updated").unwrap(),
            row.get("last_fee").unwrap(),
            row.get("last_revenue").unwrap(),
        ))
    })
    .map(|row| row.unwrap())
    .collect()
}
