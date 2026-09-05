use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::db::{Database, DatabaseError, Trader};

const RUN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct TraderRuntime {
    database: Database,
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Template {
    pub id: &'static str,
    pub exchange: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub params_example: Value,
    pub signal_example: Value,
}

impl TraderRuntime {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Reconciles in-memory runtime tasks with the durable enabled state.
    pub async fn reconcile(&self) {
        let Ok(enabled) = self.database.enabled_traders() else {
            return;
        };
        let enabled_ids = enabled
            .iter()
            .map(|trader| trader.id.clone())
            .collect::<HashSet<_>>();
        let mut tasks = self.tasks.lock().await;
        tasks.retain(|id, task| {
            if enabled_ids.contains(id) {
                true
            } else {
                task.abort();
                false
            }
        });
        for trader in enabled {
            if tasks.contains_key(&trader.id) {
                continue;
            }
            let database = self.database.clone();
            let trader_id = trader.id.clone();
            tasks.insert(
                trader.id,
                tokio::spawn(async move {
                    loop {
                        let Some(trader) = database.get_trader(&trader_id).ok().flatten() else {
                            return;
                        };
                        if !trader.enabled {
                            return;
                        }
                        run_trader(&database, &trader).await;
                        tokio::time::sleep(RUN_INTERVAL).await;
                    }
                }),
            );
        }
    }
}

pub fn templates() -> Vec<Template> {
    vec![
        Template {
            id: "copy_target_position.binance.um_futures.20260605",
            exchange: "binance",
            title: "Binance UM 目标仓位",
            description: "市价调整到单向或指定持仓方向的目标数量。",
            params_example: json!({"product_id":"BTCUSDT","tolerance_qty":"0.001","max_order_qty":"0.01"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction.binance.um_futures.20260605",
            exchange: "binance",
            title: "Binance UM BBO 分方向挂单",
            description: "以最优买卖价（BBO）为每个方向维持目标仓位。",
            params_example: json!({"product_id":"BTCUSDT","tolerance_qty":"0.001","max_order_qty":"0.01"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position.okx.swap.20260605",
            exchange: "okx",
            title: "OKX Swap 目标仓位",
            description: "将永续合约净仓位调整到目标数量。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker.okx.swap.20260605",
            exchange: "okx",
            title: "OKX Swap BBO 挂单",
            description: "以最优价挂单方式追踪单一目标仓位。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction.okx.swap.20260605",
            exchange: "okx",
            title: "OKX Swap BBO 分方向挂单",
            description: "在多空分持仓模式下分别追踪多头与空头目标。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction_singleflight.okx.swap.20260609",
            exchange: "okx",
            title: "OKX Swap BBO 单飞行分方向",
            description: "同账户共享一次持仓查询的分方向 BBO 执行。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position_multi_order_maker.okx.swap.20260605",
            exchange: "okx",
            title: "OKX Swap 多单挂单",
            description: "将净目标仓位拆分为多笔 maker 挂单。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10","order_count":3}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_multi_order_maker_by_direction.okx.swap.20260605",
            exchange: "okx",
            title: "OKX Swap 多单分方向挂单",
            description: "将多空目标分别拆分为多笔 maker 挂单。",
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10","order_count":3}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position.ctpd.cffex_index_futures.20260719",
            exchange: "ctpd",
            title: "CTPD 中金所股指期货对冲优先",
            description: "IF、IH、IC、IM 的昨仓优先平仓、对冲开仓和今仓平仓执行。",
            params_example: json!({"instrument_id":"IF2609","buy_price":"4000","sell_price":"3999"}),
            signal_example: json!({"target_long_volume":0,"target_short_volume":0}),
        },
    ]
}

pub fn template(template_id: &str) -> Option<Template> {
    templates()
        .into_iter()
        .find(|template| template.id == template_id)
}

pub fn validate_configuration(
    template_id: &str,
    credential_id: &str,
    params: &Value,
    signal: &Value,
) -> Result<(), String> {
    let Some(_) = template(template_id) else {
        return Err("unknown trader template".into());
    };
    let config = merged_config(credential_id, params, signal)?;
    serde_json::from_value::<traders::TraderModel>(json!({"model": template_id, "config": config}))
        .map(|_| ())
        .map_err(|error| format!("template configuration is invalid: {error}"))
}

async fn run_trader(database: &Database, trader: &Trader) {
    let started_at = chrono::Utc::now().timestamp();
    let result = execute(database, trader).await;
    match result {
        Ok(summary) => {
            let _ = database.record_run(&trader.id, "succeeded", Some(&summary), started_at);
        }
        Err(error) => {
            let _ = database.record_run(&trader.id, "failed", Some(&error), started_at);
            notify_linkit(database, trader, &error).await;
        }
    }
}

async fn execute(database: &Database, trader: &Trader) -> Result<String, String> {
    let credential = database
        .secret_credential(&trader.credential_id)
        .map_err(|error| database_error(&error))?
        .ok_or_else(|| "credential does not exist".to_owned())?;
    let expected_exchange = template(&trader.template_id)
        .ok_or_else(|| "unknown trader template".to_owned())?
        .exchange;
    if credential.exchange != expected_exchange {
        return Err("credential exchange does not match trader template".into());
    }
    let config = merged_config(&trader.credential_id, &trader.params, &trader.signal)?;
    let model: traders::TraderModel =
        serde_json::from_value(json!({"model": trader.template_id, "config": config}))
            .map_err(|error| format!("template configuration is invalid: {error}"))?;
    let credential: traders::AccountCredential = serde_json::from_value(credential.json)
        .map_err(|_| "credential format does not match its exchange".to_owned())?;
    match model {
        traders::TraderModel::BinanceUmFuturesCopyTargetPositionV1(config) => {
            traders::run_binance_um_futures_copy_target_position_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::BinanceUmFuturesCopyTargetPositionBboMakerByDirection20260605(
            config,
        ) => traders::run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once(
            &config,
            &credential,
        )
        .await
        .map(|run| format!("{run:?}")),
        traders::TraderModel::CtpdCffexIndexFuturesHedgePriority20260719(config) => {
            traders::run_ctpd_cffex_index_futures_hedge_priority_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::OkxSwapCopyTargetPositionV1(config) => {
            traders::run_okx_swap_copy_target_position_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::OkxSwapCopyTargetPositionBboMaker20260605(config) => {
            traders::run_okx_swap_copy_target_position_bbo_maker_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::OkxSwapCopyTargetPositionBboMakerByDirection20260605(config) => {
            traders::run_okx_swap_copy_target_position_bbo_maker_by_direction_once(
                &config,
                &credential,
            )
            .await
            .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::OkxSwapCopyTargetPositionBboMakerByDirectionSingleflight20260609(
            config,
        ) => traders::run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once(
            &config,
            &credential,
        )
        .await
        .map(|run| format!("{run:?}")),
        traders::TraderModel::OkxSwapCopyTargetPositionMultiOrderMaker20260605(config) => {
            traders::run_okx_swap_copy_target_position_multi_order_maker_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        traders::TraderModel::OkxSwapCopyTargetPositionMultiOrderMakerByDirection20260605(
            config,
        ) => traders::run_okx_swap_copy_target_position_multi_order_maker_by_direction_once(
            &config,
            &credential,
        )
        .await
        .map(|run| format!("{run:?}")),
    }
    .map_err(|error| format!("execution failed: {error}"))
}

fn merged_config(credential_id: &str, params: &Value, signal: &Value) -> Result<Value, String> {
    let Some(mut params) = params.as_object().cloned() else {
        return Err("params must be a JSON object".into());
    };
    let Some(signal) = signal.as_object() else {
        return Err("signal must be a JSON object".into());
    };
    params.extend(signal.clone());
    params.insert("account_id".into(), Value::String(credential_id.to_owned()));
    Ok(Value::Object(params))
}

async fn notify_linkit(database: &Database, trader: &Trader, error: &str) {
    let Ok(Some(settings)) = database.secret_linkit_settings(&trader.owner_id) else {
        return;
    };
    let _ = reqwest::Client::new().post("https://linkit.ntnl.io/bot/v1/messages")
        .bearer_auth(settings.bot_token)
        .json(&json!({"recipient_username": settings.recipient_username, "body": format!("HIT 交易者「{}」执行失败：{}", trader.name, error)}))
        .send().await;
}

fn database_error(error: &DatabaseError) -> String {
    format!("state storage failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::validate_configuration;
    use serde_json::json;

    #[test]
    fn accepts_separated_binance_config() {
        assert!(
            validate_configuration(
                "copy_target_position.binance.um_futures.20260605",
                "credential-id",
                &json!({"product_id":"BTCUSDT","tolerance_qty":"0.001","max_order_qty":"0.01"}),
                &json!({"target_qty":"0"})
            )
            .is_ok()
        );
    }

    #[test]
    fn rejects_unknown_template() {
        assert!(validate_configuration("nope", "credential-id", &json!({}), &json!({})).is_err());
    }
}
