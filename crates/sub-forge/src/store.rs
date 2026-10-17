mod migrations;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use rusqlite::{Connection, OptionalExtension, params};
use uuid::Uuid;

use crate::{error::AppError, model::StoredLink};

#[derive(Clone)]
pub struct Store {
    connection: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS links (
                 id TEXT PRIMARY KEY NOT NULL,
                 payload TEXT NOT NULL UNIQUE,
                 created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
             );",
        )?;
        migrations::apply(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    pub async fn save(&self, record: StoredLink) -> Result<String, AppError> {
        let payload = serde_json::to_string(&record).map_err(|_| AppError::Internal)?;
        self.run(move |connection| {
            // 随机 ID 不携带配置；相同内容复用记录，刷新和请求重试不会持续增加数据。
            let id = Uuid::new_v4().simple().to_string();
            connection.execute(
                "INSERT INTO links (id, payload) VALUES (?1, ?2) ON CONFLICT(payload) DO NOTHING",
                params![id, payload],
            ).map_err(database_error)?;
            connection
                .query_row(
                    "SELECT id FROM links WHERE payload = ?1",
                    [payload],
                    |row| row.get(0),
                )
                .map_err(database_error)
        })
        .await
    }

    pub async fn load(&self, id: &str) -> Result<StoredLink, AppError> {
        if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(AppError::Unauthorized);
        }
        let id = id.to_owned();
        self.run(move |connection| {
            let payload: String = connection
                .query_row("SELECT payload FROM links WHERE id = ?1", [id], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(database_error)?
                .ok_or(AppError::Unauthorized)?;
            serde_json::from_str(&payload).map_err(|_| AppError::Internal)
        })
        .await
    }

    async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
    ) -> Result<T, AppError> {
        let connection = self.connection.clone();
        // SQLite 的锁等待和磁盘 I/O 不占用 Tokio 异步工作线程。
        tokio::task::spawn_blocking(move || {
            let connection = connection.lock().map_err(|_| AppError::Internal)?;
            operation(&connection)
        })
        .await
        .map_err(|_| AppError::Internal)?
    }
}

fn database_error(error: rusqlite::Error) -> AppError {
    // 不记录可能包含订阅、脚本或凭据的 SQL 参数。
    tracing::error!(code = ?error.sqlite_error_code(), "配置数据库操作失败");
    AppError::Internal
}
