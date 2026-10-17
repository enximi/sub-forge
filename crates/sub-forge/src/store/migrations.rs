use rusqlite::Connection;
use serde_json::{Value, json};

use crate::model::StoredLink;

pub fn apply(connection: &mut Connection) -> Result<(), Box<dyn std::error::Error>> {
    let transaction = connection.transaction()?;
    let version: u32 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    match version {
        0 => {
            let rows = {
                let mut query = transaction.prepare("SELECT id, payload FROM links")?;
                query
                    .query_map([], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            // 首次升级将已有记录转换为按用途区分的结构，保留 ID 和创建时间。
            for (id, payload) in rows {
                let old: Value = serde_json::from_str(&payload)?;
                let (kind, config) = match old["scope"].as_str() {
                    Some("Feed") => (
                        "feed",
                        json!({
                            "source_url": old["source_url"], "converter_url": old["converter_url"],
                            "policy": old["policy"], "scripts": old["scripts"]
                        }),
                    ),
                    Some("Subscription") => (
                        "subscription",
                        json!({"source_url": old["source_url"], "script": old["script"]}),
                    ),
                    Some("Rules") => (
                        "rules",
                        json!({"source_url": old["source_url"], "script": old["script"]}),
                    ),
                    Some("RuleResource") => {
                        ("rule_resource", json!({"source_url": old["source_url"]}))
                    }
                    _ => return Err("unsupported stored link format".into()),
                };
                let record: StoredLink =
                    serde_json::from_value(json!({"kind": kind, "config": config}))?;
                transaction.execute(
                    "UPDATE links SET payload = ?1 WHERE id = ?2",
                    [serde_json::to_string(&record)?, id],
                )?;
            }
            transaction.pragma_update(None, "user_version", 1)?;
        }
        1 => {}
        _ => return Err("database schema is newer than this application".into()),
    }
    transaction.commit()?;
    Ok(())
}
