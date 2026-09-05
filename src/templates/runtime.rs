use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraderSpec {
    pub trader_id: i64,
    pub model: String,
    pub config: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TraderSpecRequest {
    pub trader_id: Option<i64>,
    pub model: String,
    pub config: Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceClaim {
    pub key: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum TraderStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}
