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
        1 | 2 => {}
        _ => return Err("database schema is newer than this application".into()),
    }
    if version < 2 {
        // 可编辑订阅独立保存，仅不可变的取源和规则回调按内容去重。
        transaction.execute_batch(
            "CREATE TABLE links_next (
                 id TEXT PRIMARY KEY NOT NULL,
                 payload TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );
             INSERT INTO links_next SELECT id, payload, created_at FROM links;
             DROP TABLE links;
             ALTER TABLE links_next RENAME TO links;
             CREATE UNIQUE INDEX links_callback_payload ON links(payload)
                 WHERE json_extract(payload, '$.kind') != 'feed';
             PRAGMA user_version = 2;",
        )?;
    }
    transaction.commit()?;
    Ok(())
}
