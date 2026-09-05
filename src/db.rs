use std::path::Path;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{Cipher, CipherError};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    cipher: Cipher,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Database").finish_non_exhaustive()
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite operation failed")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database lock is poisoned")]
    Poisoned,
    #[error("credential encryption failed")]
    Cipher(#[from] CipherError),
    #[error("stored JSON is invalid")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Credential {
    pub id: String,
    pub owner_id: String,
    pub exchange: String,
    pub label: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Trader {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub template_id: String,
    pub credential_id: String,
    pub params: Value,
    pub signal: Value,
    pub signal_token_prefix: String,
    pub enabled: bool,
    pub status: String,
    pub last_run_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Run {
    pub id: String,
    pub trader_id: String,
    pub status: String,
    pub summary: Option<String>,
    pub started_at: i64,
    pub finished_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LinkitSettings {
    pub owner_id: String,
    pub recipient_username: String,
    pub configured: bool,
    pub updated_at: i64,
}

#[derive(Clone, Debug)]
pub struct SecretCredential {
    pub exchange: String,
    pub json: Value,
}

#[derive(Clone, Debug)]
pub struct SecretLinkitSettings {
    pub recipient_username: String,
    pub bot_token: String,
}

impl Database {
    /// Opens HIT's `SQLite` database and forces WAL mode before serving requests.
    ///
    /// # Errors
    ///
    /// Returns an error when the database, schema or credential cipher cannot be initialized.
    pub fn open(state_directory: impl AsRef<Path>) -> Result<Self, DatabaseError> {
        let state_directory = state_directory.as_ref();
        let cipher = Cipher::load_or_create(state_directory)?;
        let connection = Connection::open(state_directory.join("default.sqlite3"))?;
        connection.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA busy_timeout = 5000;
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS credentials (
                id TEXT PRIMARY KEY NOT NULL,
                owner_id TEXT NOT NULL,
                exchange TEXT NOT NULL,
                label TEXT NOT NULL,
                secret_ciphertext TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS credentials_owner_id_idx ON credentials(owner_id);
            CREATE TABLE IF NOT EXISTS traders (
                id TEXT PRIMARY KEY NOT NULL,
                owner_id TEXT NOT NULL,
                name TEXT NOT NULL,
                template_id TEXT NOT NULL,
                credential_id TEXT NOT NULL REFERENCES credentials(id),
                params_json TEXT NOT NULL,
                signal_json TEXT NOT NULL,
                signal_token_hash TEXT NOT NULL,
                signal_token_prefix TEXT NOT NULL,
                enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
                status TEXT NOT NULL,
                last_run_at INTEGER,
                last_error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS traders_owner_id_idx ON traders(owner_id);
            CREATE TABLE IF NOT EXISTS trader_runs (
                id TEXT PRIMARY KEY NOT NULL,
                trader_id TEXT NOT NULL REFERENCES traders(id) ON DELETE CASCADE,
                status TEXT NOT NULL,
                summary TEXT,
                started_at INTEGER NOT NULL,
                finished_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS trader_runs_trader_id_idx ON trader_runs(trader_id, started_at DESC);
            CREATE TABLE IF NOT EXISTS linkit_settings (
                owner_id TEXT PRIMARY KEY NOT NULL,
                recipient_username TEXT NOT NULL,
                bot_token_ciphertext TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher,
        })
    }

    pub fn root_user_id(&self) -> Result<Option<String>, DatabaseError> {
        self.connection()?
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'root_user_id'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn initialize_root_user(&self, user_id: &str) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "INSERT INTO app_meta(key, value) VALUES ('root_user_id', ?1) ON CONFLICT(key) DO NOTHING",
            [user_id],
        )?;
        Ok(changed == 1)
    }

    pub fn list_credentials(
        &self,
        owner_id: Option<&str>,
    ) -> Result<Vec<Credential>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = if owner_id.is_some() {
            connection.prepare(
                "SELECT id, owner_id, exchange, label, created_at, updated_at FROM credentials WHERE owner_id = ?1 ORDER BY updated_at DESC",
            )?
        } else {
            connection.prepare(
                "SELECT id, owner_id, exchange, label, created_at, updated_at FROM credentials ORDER BY updated_at DESC",
            )?
        };
        let rows = match owner_id {
            Some(owner_id) => statement.query_map([owner_id], credential_from_row)?,
            None => statement.query_map([], credential_from_row)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn get_credential(&self, id: &str) -> Result<Option<Credential>, DatabaseError> {
        self.connection()?
            .query_row(
                "SELECT id, owner_id, exchange, label, created_at, updated_at FROM credentials WHERE id = ?1",
                [id],
                credential_from_row,
            )
            .optional()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn create_credential(
        &self,
        owner_id: &str,
        exchange: &str,
        label: &str,
        secret: &Value,
    ) -> Result<Credential, DatabaseError> {
        let credential = Credential {
            id: Uuid::new_v4().to_string(),
            owner_id: owner_id.to_owned(),
            exchange: exchange.to_owned(),
            label: label.to_owned(),
            created_at: now(),
            updated_at: now(),
        };
        let ciphertext = self.cipher.encrypt(&serde_json::to_string(secret)?)?;
        self.connection()?.execute(
            "INSERT INTO credentials(id, owner_id, exchange, label, secret_ciphertext, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![credential.id, credential.owner_id, credential.exchange, credential.label, ciphertext, credential.created_at, credential.updated_at],
        )?;
        Ok(credential)
    }

    pub fn update_credential(
        &self,
        id: &str,
        label: &str,
        secret: &Value,
    ) -> Result<Option<Credential>, DatabaseError> {
        let ciphertext = self.cipher.encrypt(&serde_json::to_string(secret)?)?;
        let changed = self.connection()?.execute(
            "UPDATE credentials SET label = ?2, secret_ciphertext = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, label, ciphertext, now()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_credential(id)
    }

    pub fn delete_credential(&self, id: &str) -> Result<bool, DatabaseError> {
        Ok(self
            .connection()?
            .execute("DELETE FROM credentials WHERE id = ?1", [id])?
            == 1)
    }

    pub fn secret_credential(&self, id: &str) -> Result<Option<SecretCredential>, DatabaseError> {
        let row = self
            .connection()?
            .query_row(
                "SELECT exchange, secret_ciphertext FROM credentials WHERE id = ?1",
                [id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        row.map(|(exchange, ciphertext)| {
            let plaintext = self.cipher.decrypt(&ciphertext)?;
            Ok(SecretCredential {
                exchange,
                json: serde_json::from_str(&plaintext)?,
            })
        })
        .transpose()
    }

    pub fn list_traders(&self, owner_id: Option<&str>) -> Result<Vec<Trader>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = if owner_id.is_some() {
            connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, created_at, updated_at FROM traders WHERE owner_id = ?1 ORDER BY updated_at DESC")?
        } else {
            connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, created_at, updated_at FROM traders ORDER BY updated_at DESC")?
        };
        let rows = match owner_id {
            Some(owner_id) => statement.query_map([owner_id], trader_from_row)?,
            None => statement.query_map([], trader_from_row)?,
        };
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn enabled_traders(&self) -> Result<Vec<Trader>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, created_at, updated_at FROM traders WHERE enabled = 1 ORDER BY updated_at DESC")?;
        statement
            .query_map([], trader_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn get_trader(&self, id: &str) -> Result<Option<Trader>, DatabaseError> {
        self.connection()?.query_row(
            "SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, created_at, updated_at FROM traders WHERE id = ?1", [id], trader_from_row,
        ).optional().map_err(DatabaseError::Sqlite)
    }

    pub fn create_trader(&self, draft: &TraderDraft) -> Result<(Trader, String), DatabaseError> {
        let token = format!("sk-{}", Uuid::new_v4().simple());
        let trader = Trader {
            id: Uuid::new_v4().to_string(),
            owner_id: draft.owner_id.clone(),
            name: draft.name.clone(),
            template_id: draft.template_id.clone(),
            credential_id: draft.credential_id.clone(),
            params: draft.params.clone(),
            signal: draft.signal.clone(),
            signal_token_prefix: token[..11].to_owned(),
            enabled: draft.enabled,
            status: if draft.enabled {
                "starting".into()
            } else {
                "stopped".into()
            },
            last_run_at: None,
            last_error: None,
            created_at: now(),
            updated_at: now(),
        };
        self.connection()?.execute(
            "INSERT INTO traders(id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_hash, signal_token_prefix, enabled, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![trader.id, trader.owner_id, trader.name, trader.template_id, trader.credential_id, serde_json::to_string(&trader.params)?, serde_json::to_string(&trader.signal)?, hash_token(&token), trader.signal_token_prefix, i64::from(trader.enabled), trader.status, trader.created_at, trader.updated_at],
        )?;
        Ok((trader, token))
    }

    pub fn update_trader(
        &self,
        id: &str,
        draft: &TraderUpdate,
    ) -> Result<Option<Trader>, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE traders SET name = ?2, credential_id = ?3, params_json = ?4, signal_json = ?5, enabled = ?6, status = ?7, updated_at = ?8 WHERE id = ?1",
            params![id, draft.name, draft.credential_id, serde_json::to_string(&draft.params)?, serde_json::to_string(&draft.signal)?, i64::from(draft.enabled), if draft.enabled { "starting" } else { "stopped" }, now()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_trader(id)
    }

    pub fn set_trader_enabled(
        &self,
        id: &str,
        enabled: bool,
    ) -> Result<Option<Trader>, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE traders SET enabled = ?2, status = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id,
                i64::from(enabled),
                if enabled { "starting" } else { "stopped" },
                now()
            ],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_trader(id)
    }

    pub fn delete_trader(&self, id: &str) -> Result<bool, DatabaseError> {
        Ok(self
            .connection()?
            .execute("DELETE FROM traders WHERE id = ?1", [id])?
            == 1)
    }

    pub fn rotate_signal_token(&self, id: &str) -> Result<Option<String>, DatabaseError> {
        let token = format!("sk-{}", Uuid::new_v4().simple());
        let prefix = token[..11].to_owned();
        let changed = self.connection()?.execute("UPDATE traders SET signal_token_hash = ?2, signal_token_prefix = ?3, updated_at = ?4 WHERE id = ?1", params![id, hash_token(&token), prefix, now()])?;
        Ok((changed == 1).then_some(token))
    }

    pub fn update_signal(
        &self,
        id: &str,
        token: &str,
        signal: &Value,
    ) -> Result<bool, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE traders SET signal_json = ?3, updated_at = ?4 WHERE id = ?1 AND signal_token_hash = ?2",
            params![id, hash_token(token), serde_json::to_string(signal)?, now()],
        )?;
        Ok(changed == 1)
    }

    pub fn record_run(
        &self,
        trader_id: &str,
        status: &str,
        summary: Option<&str>,
        started_at: i64,
    ) -> Result<(), DatabaseError> {
        let finished_at = now();
        self.connection()?.execute(
            "INSERT INTO trader_runs(id, trader_id, status, summary, started_at, finished_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![Uuid::new_v4().to_string(), trader_id, status, summary, started_at, finished_at],
        )?;
        self.connection()?.execute(
            "UPDATE traders SET status = ?2, last_run_at = ?3, last_error = ?4, updated_at = ?3 WHERE id = ?1",
            params![trader_id, if status == "succeeded" { "running" } else { "failed" }, finished_at, if status == "succeeded" { None } else { summary }],
        )?;
        Ok(())
    }

    pub fn list_runs(&self, trader_id: &str) -> Result<Vec<Run>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id, trader_id, status, summary, started_at, finished_at FROM trader_runs WHERE trader_id = ?1 ORDER BY started_at DESC LIMIT 100")?;
        statement
            .query_map([trader_id], |row| {
                Ok(Run {
                    id: row.get(0)?,
                    trader_id: row.get(1)?,
                    status: row.get(2)?,
                    summary: row.get(3)?,
                    started_at: row.get(4)?,
                    finished_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn put_linkit_settings(
        &self,
        owner_id: &str,
        recipient_username: &str,
        bot_token: &str,
    ) -> Result<LinkitSettings, DatabaseError> {
        let updated_at = now();
        let ciphertext = self.cipher.encrypt(bot_token)?;
        self.connection()?.execute(
            "INSERT INTO linkit_settings(owner_id, recipient_username, bot_token_ciphertext, updated_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(owner_id) DO UPDATE SET recipient_username = excluded.recipient_username, bot_token_ciphertext = excluded.bot_token_ciphertext, updated_at = excluded.updated_at",
            params![owner_id, recipient_username, ciphertext, updated_at],
        )?;
        Ok(LinkitSettings {
            owner_id: owner_id.to_owned(),
            recipient_username: recipient_username.to_owned(),
            configured: true,
            updated_at,
        })
    }

    pub fn linkit_settings(&self, owner_id: &str) -> Result<Option<LinkitSettings>, DatabaseError> {
        self.connection()?.query_row("SELECT owner_id, recipient_username, updated_at FROM linkit_settings WHERE owner_id = ?1", [owner_id], |row| Ok(LinkitSettings { owner_id: row.get(0)?, recipient_username: row.get(1)?, configured: true, updated_at: row.get(2)? })).optional().map_err(DatabaseError::Sqlite)
    }

    pub fn secret_linkit_settings(
        &self,
        owner_id: &str,
    ) -> Result<Option<SecretLinkitSettings>, DatabaseError> {
        let row = self.connection()?.query_row("SELECT recipient_username, bot_token_ciphertext FROM linkit_settings WHERE owner_id = ?1", [owner_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).optional()?;
        row.map(|(recipient_username, ciphertext)| {
            Ok(SecretLinkitSettings {
                recipient_username,
                bot_token: self.cipher.decrypt(&ciphertext)?,
            })
        })
        .transpose()
    }

    fn connection(&self) -> Result<std::sync::MutexGuard<'_, Connection>, DatabaseError> {
        self.connection.lock().map_err(|_| DatabaseError::Poisoned)
    }
}

#[derive(Clone, Debug)]
pub struct TraderDraft {
    pub owner_id: String,
    pub name: String,
    pub template_id: String,
    pub credential_id: String,
    pub params: Value,
    pub signal: Value,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct TraderUpdate {
    pub name: String,
    pub credential_id: String,
    pub params: Value,
    pub signal: Value,
    pub enabled: bool,
}

fn credential_from_row(row: &Row<'_>) -> rusqlite::Result<Credential> {
    Ok(Credential {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        exchange: row.get(2)?,
        label: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}
fn trader_from_row(row: &Row<'_>) -> rusqlite::Result<Trader> {
    let params = serde_json::from_str(&row.get::<_, String>(5)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let signal = serde_json::from_str(&row.get::<_, String>(6)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(Trader {
        id: row.get(0)?,
        owner_id: row.get(1)?,
        name: row.get(2)?,
        template_id: row.get(3)?,
        credential_id: row.get(4)?,
        params,
        signal,
        signal_token_prefix: row.get(7)?,
        enabled: row.get::<_, i64>(8)? == 1,
        status: row.get(9)?,
        last_run_at: row.get(10)?,
        last_error: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}
fn now() -> i64 {
    Utc::now().timestamp()
}
fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use serde_json::json;
    use tempfile::tempdir;

    use super::{Database, TraderDraft};

    #[test]
    fn set_trader_enabled_preserves_trader_configuration() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let params = json!({"product_id":"BTCUSDT"});
        let signal = json!({"target_qty":"1"});
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id,
            params: params.clone(),
            signal: signal.clone(),
            enabled: false,
        })?;

        let trader = database
            .set_trader_enabled(&trader.id, true)?
            .expect("created trader exists");
        assert!(trader.enabled);
        assert_eq!(trader.status, "starting");
        assert_eq!(trader.params, params);
        assert_eq!(trader.signal, signal);

        let trader = database
            .set_trader_enabled(&trader.id, false)?
            .expect("created trader exists");
        assert!(!trader.enabled);
        assert_eq!(trader.status, "stopped");
        assert_eq!(trader.params, params);
        assert_eq!(trader.signal, signal);
        Ok(())
    }
}
