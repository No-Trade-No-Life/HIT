use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::crypto::{Cipher, CipherError};

#[derive(Clone)]
pub struct Database {
    connection: Arc<Mutex<Connection>>,
    cipher: Cipher,
    database_path: Arc<PathBuf>,
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
    #[error("legacy CTPD trader configuration is invalid")]
    LegacyCtpdTraderConfiguration,
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
    pub successful_runs: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SignalHistory {
    pub id: String,
    pub trader_id: String,
    pub signal: Value,
    pub occurrences: i64,
    pub created_at: i64,
    pub updated_at: i64,
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
        let database_path = state_directory.join("default.sqlite3");
        let mut connection = Connection::open(&database_path)?;
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
                successful_runs INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS traders_owner_id_idx ON traders(owner_id);
            CREATE TABLE IF NOT EXISTS signal_history (
                id TEXT PRIMARY KEY NOT NULL,
                trader_id TEXT NOT NULL REFERENCES traders(id) ON DELETE CASCADE,
                signal_json TEXT NOT NULL,
                occurrences INTEGER NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS signal_history_trader_id_idx ON signal_history(trader_id, updated_at DESC);
            CREATE TABLE IF NOT EXISTS linkit_settings (
                owner_id TEXT PRIMARY KEY NOT NULL,
                recipient_username TEXT NOT NULL,
                bot_token_ciphertext TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )?;
        if !has_successful_runs_column(&connection)? {
            connection.execute(
                "ALTER TABLE traders ADD COLUMN successful_runs INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        migrate_ctpd_bbo_taker_configuration(&mut connection)?;
        connection.execute_batch("DROP TABLE IF EXISTS trader_runs;")?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            cipher,
            database_path: Arc::new(database_path),
        })
    }

    pub fn database_path(&self) -> PathBuf {
        self.database_path.as_ref().clone()
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
            connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE owner_id = ?1 ORDER BY updated_at DESC")?
        } else {
            connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders ORDER BY updated_at DESC")?
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
        let mut statement = connection.prepare("SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE enabled = 1 ORDER BY updated_at DESC")?;
        statement
            .query_map([], trader_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(DatabaseError::Sqlite)
    }

    pub fn get_trader(&self, id: &str) -> Result<Option<Trader>, DatabaseError> {
        self.connection()?.query_row(
            "SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE id = ?1", [id], trader_from_row,
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
            successful_runs: 0,
            created_at: now(),
            updated_at: now(),
        };
        self.connection()?.execute(
            "INSERT INTO traders(id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_hash, signal_token_prefix, enabled, status, successful_runs, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![trader.id, trader.owner_id, trader.name, trader.template_id, trader.credential_id, serde_json::to_string(&trader.params)?, serde_json::to_string(&trader.signal)?, hash_token(&token), trader.signal_token_prefix, i64::from(trader.enabled), trader.status, trader.successful_runs, trader.created_at, trader.updated_at],
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

    pub fn set_trader_params(
        &self,
        id: &str,
        params: &Value,
    ) -> Result<Option<Trader>, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE traders SET params_json = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, serde_json::to_string(params)?, now()],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        self.get_trader(id)
    }

    pub fn set_trader_name(&self, id: &str, name: &str) -> Result<Option<Trader>, DatabaseError> {
        let changed = self.connection()?.execute(
            "UPDATE traders SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, now()],
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

    pub fn set_trader_signal(
        &self,
        id: &str,
        signal: &Value,
    ) -> Result<Option<Trader>, DatabaseError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let trader = transaction
            .query_row(
                "SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE id = ?1",
                [id],
                trader_from_row,
            )
            .optional()?;
        let Some(trader) = trader else {
            return Ok(None);
        };
        let trader = record_signal_patch(&transaction, trader, signal)?;
        transaction.commit()?;
        Ok(Some(trader))
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
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let trader = transaction
            .query_row(
                "SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE id = ?1 AND signal_token_hash = ?2",
                params![id, hash_token(token)],
                trader_from_row,
            )
            .optional()?;
        let Some(trader) = trader else {
            return Ok(false);
        };
        record_signal_patch(&transaction, trader, signal)?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn record_successful_run(&self, trader_id: &str) -> Result<(), DatabaseError> {
        let completed_at = now();
        self.connection()?.execute(
            "UPDATE traders SET successful_runs = successful_runs + 1, status = 'running', last_run_at = ?2, last_error = NULL, updated_at = ?2 WHERE id = ?1",
            params![trader_id, completed_at],
        )?;
        Ok(())
    }

    pub fn record_failed_run(&self, trader_id: &str, error: &str) -> Result<(), DatabaseError> {
        let completed_at = now();
        self.connection()?.execute(
            "UPDATE traders SET status = 'failed', last_run_at = ?2, last_error = ?3, updated_at = ?2 WHERE id = ?1",
            params![trader_id, completed_at, error],
        )?;
        Ok(())
    }

    pub fn list_signal_history(
        &self,
        trader_id: &str,
    ) -> Result<Vec<SignalHistory>, DatabaseError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare("SELECT id, trader_id, signal_json, occurrences, created_at, updated_at FROM signal_history WHERE trader_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 100")?;
        statement
            .query_map([trader_id], signal_history_from_row)?
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

fn signal_history_from_row(row: &Row<'_>) -> rusqlite::Result<SignalHistory> {
    let signal = serde_json::from_str(&row.get::<_, String>(2)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(SignalHistory {
        id: row.get(0)?,
        trader_id: row.get(1)?,
        signal,
        occurrences: row.get(3)?,
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
        successful_runs: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn record_signal_patch(
    transaction: &rusqlite::Transaction<'_>,
    trader: Trader,
    signal: &Value,
) -> Result<Trader, DatabaseError> {
    let recorded_at = now();
    let latest = transaction
        .query_row(
            "SELECT id, trader_id, signal_json, occurrences, created_at, updated_at FROM signal_history WHERE trader_id = ?1 ORDER BY updated_at DESC, rowid DESC LIMIT 1",
            [&trader.id],
            signal_history_from_row,
        )
        .optional()?;
    if let Some(latest) = latest.filter(|entry| trader.signal == *signal && entry.signal == *signal)
    {
        transaction.execute(
            "UPDATE signal_history SET occurrences = occurrences + 1, updated_at = ?2 WHERE id = ?1",
            params![latest.id, recorded_at],
        )?;
        return Ok(trader);
    }
    transaction.execute(
        "INSERT INTO signal_history(id, trader_id, signal_json, occurrences, created_at, updated_at) VALUES (?1, ?2, ?3, 1, ?4, ?4)",
        params![Uuid::new_v4().to_string(), trader.id, serde_json::to_string(signal)?, recorded_at],
    )?;
    if trader.signal == *signal {
        return Ok(trader);
    }
    transaction.execute(
        "UPDATE traders SET signal_json = ?2, updated_at = ?3 WHERE id = ?1",
        params![trader.id, serde_json::to_string(signal)?, recorded_at],
    )?;
    transaction
        .query_row(
            "SELECT id, owner_id, name, template_id, credential_id, params_json, signal_json, signal_token_prefix, enabled, status, last_run_at, last_error, successful_runs, created_at, updated_at FROM traders WHERE id = ?1",
            [&trader.id],
            trader_from_row,
        )
        .map_err(DatabaseError::Sqlite)
}

fn migrate_ctpd_bbo_taker_configuration(connection: &mut Connection) -> Result<(), DatabaseError> {
    const TEMPLATE_ID: &str = "copy_target_position.ctpd.cffex_index_futures.20260719";
    const MIGRATION_KEY: &str = "ctpd_bbo_taker_configuration_v1";
    let transaction = connection.transaction()?;
    let already_migrated = transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_meta WHERE key = ?1)",
        [MIGRATION_KEY],
        |row| row.get::<_, i64>(0).map(|value| value != 0),
    )?;
    if already_migrated {
        return Ok(());
    }
    let mut statement = transaction
        .prepare("SELECT id, params_json, signal_json FROM traders WHERE template_id = ?1")?;
    let traders = statement
        .query_map([TEMPLATE_ID], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, params_json, signal_json) in traders {
        let mut signal = serde_json::from_str::<Map<String, Value>>(&signal_json)?;
        if signal.contains_key("net_volume") {
            continue;
        }
        let target_long_volume = signal
            .remove("target_long_volume")
            .and_then(|value| value.as_i64())
            .ok_or(DatabaseError::LegacyCtpdTraderConfiguration)?;
        let target_short_volume = signal
            .remove("target_short_volume")
            .and_then(|value| value.as_i64())
            .ok_or(DatabaseError::LegacyCtpdTraderConfiguration)?;
        let net_volume = target_long_volume
            .checked_sub(target_short_volume)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or(DatabaseError::LegacyCtpdTraderConfiguration)?;
        let mut params = serde_json::from_str::<Map<String, Value>>(&params_json)?;
        params.remove("buy_price");
        params.remove("sell_price");
        transaction.execute(
            "UPDATE traders SET params_json = ?2, signal_json = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id,
                serde_json::to_string(&params)?,
                serde_json::to_string(&serde_json::json!({"net_volume":net_volume}))?,
                now()
            ],
        )?;
    }
    // COMPATIBILITY: this converts CTPD signals written before the BBO-taker release once.
    // Remove it when pre-BBO-taker databases are no longer a supported upgrade source; verify
    // every supported source has the app_meta marker before removal.
    transaction.execute(
        "INSERT INTO app_meta(key, value) VALUES (?1, 'complete')",
        [MIGRATION_KEY],
    )?;
    transaction.commit()?;
    Ok(())
}

fn has_successful_runs_column(connection: &Connection) -> rusqlite::Result<bool> {
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('traders') WHERE name = 'successful_runs')",
        [],
        |row| row.get::<_, i64>(0).map(|exists| exists != 0),
    )
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

    use rusqlite::{Connection, OptionalExtension};
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

    #[test]
    fn set_trader_signal_preserves_execution_configuration() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let params = json!({"product_id":"BTCUSDT","max_order_qty":"0.01"});
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id.clone(),
            params: params.clone(),
            signal: json!({"target_qty":"1"}),
            enabled: true,
        })?;
        let signal = json!({"target_qty":"2"});

        let updated = database
            .set_trader_signal(&trader.id, &signal)?
            .expect("created trader exists");

        assert_eq!(updated.signal, signal);
        assert_eq!(updated.params, params);
        assert_eq!(updated.credential_id, credential.id);
        assert!(updated.enabled);
        assert_eq!(updated.status, "starting");
        Ok(())
    }

    #[test]
    fn set_trader_params_preserves_target_signal_and_runtime_state() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let signal = json!({"target_qty":"1"});
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id.clone(),
            params: json!({"product_id":"BTCUSDT","max_order_qty":"0.01"}),
            signal: signal.clone(),
            enabled: true,
        })?;
        let params = json!({"product_id":"BTCUSDT","max_order_qty":"0.02"});

        let updated = database
            .set_trader_params(&trader.id, &params)?
            .expect("created trader exists");

        assert_eq!(updated.params, params);
        assert_eq!(updated.signal, signal);
        assert_eq!(updated.credential_id, credential.id);
        assert!(updated.enabled);
        assert_eq!(updated.status, "starting");
        Ok(())
    }

    #[test]
    fn set_trader_name_preserves_execution_configuration_and_runtime_state()
    -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let params = json!({"product_id":"BTCUSDT","max_order_qty":"0.01"});
        let signal = json!({"target_qty":"1"});
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id.clone(),
            params: params.clone(),
            signal: signal.clone(),
            enabled: true,
        })?;

        let updated = database
            .set_trader_name(&trader.id, "production alpha")?
            .expect("created trader exists");

        assert_eq!(updated.name, "production alpha");
        assert_eq!(updated.params, params);
        assert_eq!(updated.signal, signal);
        assert_eq!(updated.credential_id, credential.id);
        assert!(updated.enabled);
        assert_eq!(updated.status, "starting");
        Ok(())
    }

    #[test]
    fn opening_an_existing_database_migrates_ctpd_bbo_taker_configuration()
    -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "ctpd", "primary", &json!({}))?;
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "ctp".into(),
            template_id: "copy_target_position.ctpd.cffex_index_futures.20260719".into(),
            credential_id: credential.id,
            params: json!({"instrument_id":"IF2609","buy_price":"4000","sell_price":"3999"}),
            signal: json!({"target_long_volume":3,"target_short_volume":1}),
            enabled: false,
        })?;
        database.connection()?.execute(
            "DELETE FROM app_meta WHERE key = 'ctpd_bbo_taker_configuration_v1'",
            [],
        )?;
        drop(database);

        let database = Database::open(state_directory.path())?;
        let trader = database
            .get_trader(&trader.id)?
            .expect("created trader exists");

        assert_eq!(trader.params, json!({"instrument_id":"IF2609"}));
        assert_eq!(trader.signal, json!({"net_volume":2}));
        let migration_marker = database.connection()?.query_row(
            "SELECT value FROM app_meta WHERE key = 'ctpd_bbo_taker_configuration_v1'",
            [],
            |row| row.get::<_, String>(0),
        )?;
        assert_eq!(migration_marker, "complete");
        Ok(())
    }

    #[test]
    fn signal_patch_history_coalesces_identical_payloads() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let initial_signal = json!({"target_qty":"1"});
        let (trader, token) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id,
            params: json!({"product_id":"BTCUSDT"}),
            signal: initial_signal.clone(),
            enabled: false,
        })?;

        assert!(database.update_signal(&trader.id, &token, &initial_signal)?);
        database
            .set_trader_signal(&trader.id, &initial_signal)?
            .expect("created trader exists");
        let history = database.list_signal_history(&trader.id)?;
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].signal, initial_signal);
        assert_eq!(history[0].occurrences, 2);
        assert!(history[0].updated_at >= history[0].created_at);

        let changed_signal = json!({"target_qty":"2"});
        assert!(database.update_signal(&trader.id, &token, &changed_signal)?);
        let history = database.list_signal_history(&trader.id)?;
        assert_eq!(history.len(), 2);
        assert!(
            history
                .iter()
                .any(|entry| entry.signal == changed_signal && entry.occurrences == 1)
        );
        assert_eq!(
            database
                .get_trader(&trader.id)?
                .expect("created trader exists")
                .signal,
            changed_signal
        );
        Ok(())
    }

    #[test]
    fn successful_runs_are_counted_without_execution_history() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let database = Database::open(state_directory.path())?;
        let credential = database.create_credential("owner", "binance", "primary", &json!({}))?;
        let (trader, _) = database.create_trader(&TraderDraft {
            owner_id: "owner".into(),
            name: "alpha".into(),
            template_id: "template".into(),
            credential_id: credential.id,
            params: json!({"product_id":"BTCUSDT"}),
            signal: json!({"target_qty":"1"}),
            enabled: true,
        })?;

        database.record_successful_run(&trader.id)?;
        database.record_successful_run(&trader.id)?;
        database.record_failed_run(&trader.id, "exchange rejected order")?;
        let trader = database
            .get_trader(&trader.id)?
            .expect("created trader exists");
        assert_eq!(trader.successful_runs, 2);
        assert_eq!(trader.status, "failed");
        assert_eq!(
            trader.last_error.as_deref(),
            Some("exchange rejected order")
        );
        Ok(())
    }

    #[test]
    fn opening_an_existing_database_drops_legacy_run_history() -> Result<(), Box<dyn Error>> {
        let state_directory = tempdir()?;
        let path = state_directory.path().join("default.sqlite3");
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "
            CREATE TABLE traders (
                id TEXT PRIMARY KEY NOT NULL,
                owner_id TEXT NOT NULL,
                name TEXT NOT NULL,
                template_id TEXT NOT NULL,
                credential_id TEXT NOT NULL,
                params_json TEXT NOT NULL,
                signal_json TEXT NOT NULL,
                signal_token_hash TEXT NOT NULL,
                signal_token_prefix TEXT NOT NULL,
                enabled INTEGER NOT NULL,
                status TEXT NOT NULL,
                last_run_at INTEGER,
                last_error TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE trader_runs (
                id TEXT PRIMARY KEY NOT NULL,
                trader_id TEXT NOT NULL,
                status TEXT NOT NULL,
                summary TEXT,
                started_at INTEGER NOT NULL,
                finished_at INTEGER NOT NULL
            );
            ",
        )?;
        drop(connection);

        let database = Database::open(state_directory.path())?;
        let connection = database.connection()?;
        let successful_runs_column = connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('traders') WHERE name = 'successful_runs'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        let legacy_history_table = connection
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'trader_runs'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        assert_eq!(successful_runs_column, 1);
        assert_eq!(legacy_history_table, None);
        Ok(())
    }
}
