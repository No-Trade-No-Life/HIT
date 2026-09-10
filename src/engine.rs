use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::db::{Database, DatabaseError, OkxSwapTargetLeverageState, Trader};
use crate::templates as trader_templates;

const RUN_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct TraderRuntime {
    database: Database,
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub credential_type: &'static str,
    pub exchange: &'static str,
    pub description: &'static str,
    pub params_schema: Value,
    pub signal_schema: Value,
    pub params_example: Value,
    pub signal_example: Value,
}

const BINANCE_UM_FUTURES_CREDENTIAL: &str = "binance_um_futures.api_key_secret_v1";
const OKX_CREDENTIAL: &str = "okx.api_key_secret_passphrase_v1";
const CTPD_CREDENTIAL: &str = "ctpd.http_api_key_v1";
const OKX_TARGET_LEVERAGE_TEMPLATE: &str = "target_leverage_bbo_post_only.okx.swap.20260910";

fn object_schema(title: &str, description: &str, required: &[&str], properties: Value) -> Value {
    let mut schema = serde_json::Map::new();
    schema.insert(
        "$schema".into(),
        Value::String("https://json-schema.org/draft/2020-12/schema".into()),
    );
    schema.insert("type".into(), Value::String("object".into()));
    schema.insert("title".into(), Value::String(title.to_owned()));
    schema.insert("description".into(), Value::String(description.to_owned()));
    schema.insert("additionalProperties".into(), Value::Bool(false));
    schema.insert(
        "required".into(),
        Value::Array(
            required
                .iter()
                .map(|name| Value::String((*name).to_owned()))
                .collect(),
        ),
    );
    schema.insert("properties".into(), properties);
    Value::Object(schema)
}

fn string_schema(title: &str, description: &str) -> Value {
    json!({"type":"string","title":title,"description":description})
}

fn decimal_schema(title: &str, description: &str) -> Value {
    json!({
        "type": "string",
        "title": title,
        "description": description,
        "pattern": "^-?\\d+(?:\\.\\d+)?$",
    })
}

fn integer_schema(title: &str, description: &str, minimum: i32) -> Value {
    json!({"type":"integer","title":title,"description":description,"minimum":minimum})
}

fn enum_schema(title: &str, description: &str, values: &[&str]) -> Value {
    json!({"type":"string","title":title,"description":description,"enum":values})
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
            name: "Binance UM 目标仓位",
            credential_type: BINANCE_UM_FUTURES_CREDENTIAL,
            exchange: "binance",
            description: "市价调整到单向或指定持仓方向的目标数量。",
            params_schema: object_schema(
                "Binance UM 目标仓位执行参数",
                "HIT 根据所选交易凭证自动注入账户标识。",
                &["product_id", "tolerance_qty"],
                json!({
                    "product_id": string_schema("合约", "Binance UM Futures 合约标识，例如 BTCUSDT。"),
                    "tolerance_qty": decimal_schema("仓位容差", "当前仓位与目标仓位的允许差值；差值不超过此值时不下单。"),
                    "max_order_qty": decimal_schema("单笔最大数量", "可选。限制每次市价调整的最大数量。"),
                    "account_api": enum_schema("账户 API", "可选。选择标准合约账户或组合保证金账户 API。", &["Fapi", "PortfolioMargin"]),
                    "position_side": enum_schema("持仓方向", "可选。单向模式使用 BOTH；对冲模式可指定 LONG 或 SHORT。", &["BOTH", "LONG", "SHORT"]),
                }),
            ),
            signal_schema: object_schema(
                "Binance UM 目标仓位信号",
                "外部信号提供希望达到的净仓位数量。",
                &["target_qty"],
                json!({
                    "target_qty": decimal_schema("目标仓位数量", "期望的带符号仓位数量；正数为多头，负数为净空头。"),
                }),
            ),
            params_example: json!({"product_id":"BTCUSDT","tolerance_qty":"0.001","max_order_qty":"0.01"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction.binance.um_futures.20260605",
            name: "Binance UM BBO 分方向挂单",
            credential_type: BINANCE_UM_FUTURES_CREDENTIAL,
            exchange: "binance",
            description: "以最优买卖价（BBO）为每个方向维持目标仓位。",
            params_schema: object_schema(
                "Binance UM BBO 分方向执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 Binance 对冲持仓模式。",
                &["product_id", "tolerance_qty"],
                json!({
                    "product_id": string_schema("合约", "Binance UM Futures 合约标识，例如 BTCUSDT。"),
                    "tolerance_qty": decimal_schema("仓位容差", "每个方向当前仓位与目标仓位的允许差值。"),
                    "max_order_qty": decimal_schema("单笔最大数量", "可选。限制每笔 BBO 挂单的最大数量。"),
                    "open_slippage": decimal_schema("开仓滑点", "可选。相对目标均价允许的开仓滑点。"),
                    "target_long_avg_px": decimal_schema("多头目标均价", "可选。多头开仓时使用的目标平均价格。"),
                    "target_short_avg_px": decimal_schema("空头目标均价", "可选。空头开仓时使用的目标平均价格。"),
                    "account_api": enum_schema("账户 API", "可选。选择标准合约账户或组合保证金账户 API。", &["Fapi", "PortfolioMargin"]),
                }),
            ),
            signal_schema: object_schema(
                "Binance UM 分方向目标信号",
                "分别给出多头与空头的目标绝对数量。",
                &["target_long_qty", "target_short_qty"],
                json!({
                    "target_long_qty": decimal_schema("多头目标数量", "期望保留的多头仓位绝对数量。"),
                    "target_short_qty": decimal_schema("空头目标数量", "期望保留的空头仓位绝对数量。"),
                }),
            ),
            params_example: json!({"product_id":"BTCUSDT","tolerance_qty":"0.001","max_order_qty":"0.01"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position.okx.swap.20260605",
            name: "OKX Swap 目标仓位",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "将永续合约净仓位调整到目标数量。",
            params_schema: object_schema(
                "OKX Swap 目标仓位执行参数",
                "HIT 根据所选交易凭证自动注入账户标识。",
                &["product_id", "tolerance_qty"],
                json!({
                    "product_id": string_schema("合约", "OKX 永续合约标识，例如 BTC-USDT-SWAP。"),
                    "tolerance_qty": decimal_schema("仓位容差", "当前净仓位与目标净仓位的允许差值。"),
                    "max_order_qty": decimal_schema("单笔最大数量", "可选。限制每次市价调整的最大合约数量。"),
                    "broker_code": string_schema("经纪商代码", "可选。写入 OKX 订单 tag 的经纪商代码。"),
                }),
            ),
            signal_schema: object_schema(
                "OKX Swap 目标仓位信号",
                "外部信号提供希望达到的净仓位数量。",
                &["target_qty"],
                json!({
                    "target_qty": decimal_schema("目标仓位数量", "期望的带符号净仓位数量。"),
                }),
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker.okx.swap.20260605",
            name: "OKX Swap BBO 挂单",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "以最优价挂单方式追踪单一目标仓位。",
            params_schema: object_schema(
                "OKX Swap BBO 执行参数",
                "HIT 根据所选交易凭证自动注入账户标识。",
                &["product_id", "tolerance_qty"],
                json!({
                    "product_id": string_schema("合约", "OKX 永续合约标识，例如 BTC-USDT-SWAP。"),
                    "tolerance_qty": decimal_schema("仓位容差", "当前净仓位与目标净仓位的允许差值。"),
                    "max_order_qty": decimal_schema("单笔最大数量", "可选。限制每笔最优价挂单的最大数量。"),
                    "broker_code": string_schema("经纪商代码", "可选。写入 OKX 订单 tag 的经纪商代码。"),
                }),
            ),
            signal_schema: object_schema(
                "OKX Swap BBO 目标信号",
                "外部信号提供希望通过 BBO 挂单达到的净仓位。",
                &["target_qty"],
                json!({
                    "target_qty": decimal_schema("目标仓位数量", "期望的带符号净仓位数量。"),
                }),
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: OKX_TARGET_LEVERAGE_TEMPLATE,
            name: "OKX Swap BBO Post-Only 目标杠杆",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "以账户净值计算一次目标杠杆张数；持仓后的净值漂移不再平衡，归零或反向后才重新计算。",
            params_schema: object_schema(
                "OKX Swap BBO Post-Only 目标杠杆执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 OKX net_mode，并且每个实例只管理一个永续合约。",
                &["product_id"],
                json!({
                    "product_id": string_schema("合约", "OKX 永续合约标识，例如 BTC-USDT-SWAP。"),
                    "broker_code": string_schema("经纪商代码", "可选。写入 OKX 订单 tag 的经纪商代码。"),
                }),
            ),
            signal_schema: object_schema(
                "OKX Swap 目标杠杆信号",
                "带符号的目标杠杆，定义为仓位名义价值除以账户净值；正数做多，负数做空，0 为平仓。",
                &["target_leverage"],
                json!({
                    "target_leverage": decimal_schema("目标杠杆", "仓位名义价值 / 账户净值；例如 1 为 1 倍做多，-1 为 1 倍做空，0 为平仓。"),
                }),
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP"}),
            signal_example: json!({"target_leverage":"1"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction.okx.swap.20260605",
            name: "OKX Swap BBO 分方向挂单",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "在多空分持仓模式下分别追踪多头与空头目标。",
            params_schema: directional_okx_params_schema(
                "OKX Swap BBO 分方向执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 OKX long_short_mode。",
                false,
            ),
            signal_schema: directional_signal_schema(
                "OKX Swap 分方向目标信号",
                "分别给出多头与空头的目标绝对数量。",
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position_bbo_maker_by_direction_singleflight.okx.swap.20260609",
            name: "OKX Swap BBO 单飞行分方向",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "同账户共享一次持仓查询的分方向 BBO 执行。",
            params_schema: directional_okx_params_schema(
                "OKX Swap 单飞行分方向执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 OKX long_short_mode。",
                false,
            ),
            signal_schema: directional_signal_schema(
                "OKX Swap 单飞行分方向目标信号",
                "分别给出多头与空头的目标绝对数量。",
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10"}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position_multi_order_maker.okx.swap.20260605",
            name: "OKX Swap 多单挂单",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "将净目标仓位拆分为多笔 maker 挂单。",
            params_schema: object_schema(
                "OKX Swap 多单挂单执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 OKX net_mode。",
                &["product_id", "tolerance_qty", "order_count"],
                json!({
                    "product_id": string_schema("合约", "OKX 永续合约标识，例如 BTC-USDT-SWAP。"),
                    "tolerance_qty": decimal_schema("仓位容差", "当前净仓位与目标净仓位的允许差值。"),
                    "order_count": integer_schema("挂单数量", "将目标调整拆分为的 maker 挂单数量。", 1),
                    "max_order_qty": decimal_schema("单笔最大数量", "可选。限制每笔 maker 挂单的最大数量。"),
                    "broker_code": string_schema("经纪商代码", "可选。写入 OKX 订单 tag 的经纪商代码。"),
                }),
            ),
            signal_schema: object_schema(
                "OKX Swap 多单目标信号",
                "外部信号提供希望拆单达到的净仓位。",
                &["target_qty"],
                json!({
                    "target_qty": decimal_schema("目标仓位数量", "期望的带符号净仓位数量。"),
                }),
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10","order_count":3}),
            signal_example: json!({"target_qty":"0"}),
        },
        Template {
            id: "copy_target_position_multi_order_maker_by_direction.okx.swap.20260605",
            name: "OKX Swap 多单分方向挂单",
            credential_type: OKX_CREDENTIAL,
            exchange: "okx",
            description: "将多空目标分别拆分为多笔 maker 挂单。",
            params_schema: directional_okx_params_schema(
                "OKX Swap 多单分方向执行参数",
                "HIT 根据所选交易凭证自动注入账户标识；该策略要求 OKX long_short_mode。",
                true,
            ),
            signal_schema: directional_signal_schema(
                "OKX Swap 多单分方向目标信号",
                "分别给出多头与空头的目标绝对数量。",
            ),
            params_example: json!({"product_id":"BTC-USDT-SWAP","tolerance_qty":"1","max_order_qty":"10","order_count":3}),
            signal_example: json!({"target_long_qty":"0","target_short_qty":"0"}),
        },
        Template {
            id: "copy_target_position.ctpd.cffex_index_futures.20260719",
            name: "CTPD 中金所股指期货对冲优先",
            credential_type: CTPD_CREDENTIAL,
            exchange: "ctpd",
            description: "IF、IH、IC、IM 的昨仓优先平仓、对冲开仓和今仓平仓执行。",
            params_schema: object_schema(
                "CTPD 中金所股指期货执行参数",
                "HIT 根据所选交易凭证自动注入账户标识。每笔实际委托前从 CTPD Tick 读取盘口：买入使用 AskPrice1，卖出使用 BidPrice1。",
                &["instrument_id"],
                json!({
                    "instrument_id": string_schema("合约", "中金所 IF、IH、IC 或 IM 股指期货合约，例如 IF2609。"),
                }),
            ),
            signal_schema: object_schema(
                "CTPD 中金所股指期货目标信号",
                "给出目标净头寸手数；正数为多头，负数为空头，0 为平仓。",
                &["net_volume"],
                json!({
                    "net_volume": json!({"type":"integer","title":"目标净头寸手数","description":"期望的带符号净头寸；正数为多头，负数为空头，0 为平仓。","minimum":-2147483647,"maximum":2147483647}),
                }),
            ),
            params_example: json!({"instrument_id":"IF2609"}),
            signal_example: json!({"net_volume":0}),
        },
    ]
}

fn directional_okx_params_schema(title: &str, description: &str, multi_order: bool) -> Value {
    let mut properties = serde_json::Map::from_iter([
        (
            "product_id".into(),
            string_schema("合约", "OKX 永续合约标识，例如 BTC-USDT-SWAP。"),
        ),
        (
            "tolerance_qty".into(),
            decimal_schema("仓位容差", "每个方向当前仓位与目标仓位的允许差值。"),
        ),
        (
            "max_order_qty".into(),
            decimal_schema("单笔最大数量", "可选。限制每笔 maker 挂单的最大数量。"),
        ),
        (
            "open_slippage".into(),
            decimal_schema("开仓滑点", "可选。相对目标均价允许的开仓滑点。"),
        ),
        (
            "target_long_avg_px".into(),
            decimal_schema("多头目标均价", "可选。多头开仓时使用的目标平均价格。"),
        ),
        (
            "target_short_avg_px".into(),
            decimal_schema("空头目标均价", "可选。空头开仓时使用的目标平均价格。"),
        ),
        (
            "broker_code".into(),
            string_schema("经纪商代码", "可选。写入 OKX 订单 tag 的经纪商代码。"),
        ),
    ]);
    let required = if multi_order {
        properties.insert(
            "order_count".into(),
            integer_schema(
                "挂单数量",
                "将每个方向的目标调整拆分为的 maker 挂单数量。",
                1,
            ),
        );
        vec!["product_id", "tolerance_qty", "order_count"]
    } else {
        vec!["product_id", "tolerance_qty"]
    };
    object_schema(title, description, &required, Value::Object(properties))
}

fn directional_signal_schema(title: &str, description: &str) -> Value {
    object_schema(
        title,
        description,
        &["target_long_qty", "target_short_qty"],
        json!({
            "target_long_qty": decimal_schema("多头目标数量", "期望保留的多头仓位绝对数量。"),
            "target_short_qty": decimal_schema("空头目标数量", "期望保留的空头仓位绝对数量。"),
        }),
    )
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
    serde_json::from_value::<trader_templates::TraderModel>(
        json!({"model": template_id, "config": config}),
    )
    .map(|_| ())
    .map_err(|error| format!("template configuration is invalid: {error}"))
}

async fn run_trader(database: &Database, trader: &Trader) {
    let result = execute(database, trader).await;
    match result {
        Ok(_) => {
            let _ = database.record_successful_run(&trader.id);
        }
        Err(error) => {
            let _ = database.record_failed_run(&trader.id, &error);
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
    let mut config = merged_config(&trader.credential_id, &trader.params, &trader.signal)?;
    if trader.template_id == OKX_TARGET_LEVERAGE_TEMPLATE
        && let Some(snapshot) = database
            .okx_swap_target_leverage_state(&trader.id)
            .map_err(|error| database_error(&error))?
    {
        let config = config
            .as_object_mut()
            .ok_or_else(|| "merged trader configuration must be an object".to_owned())?;
        config.insert(
            "snapshot".into(),
            serde_json::to_value(snapshot)
                .map_err(|error| format!("failed to encode target-leverage state: {error}"))?,
        );
    }
    let model: trader_templates::TraderModel =
        serde_json::from_value(json!({"model": trader.template_id, "config": config}))
            .map_err(|error| format!("template configuration is invalid: {error}"))?;
    let credential: trader_templates::AccountCredential =
        serde_json::from_value(credential.json)
            .map_err(|_| "credential format does not match its exchange".to_owned())?;
    match model {
        trader_templates::TraderModel::BinanceUmFuturesCopyTargetPositionV1(config) => {
            trader_templates::run_binance_um_futures_copy_target_position_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::BinanceUmFuturesCopyTargetPositionBboMakerByDirection20260605(
            config,
        ) => trader_templates::run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once(
            &config,
            &credential,
        )
        .await
        .map(|run| format!("{run:?}")),
        trader_templates::TraderModel::CtpdCffexIndexFuturesHedgePriority20260719(config) => {
            trader_templates::run_ctpd_cffex_index_futures_hedge_priority_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::OkxSwapCopyTargetPositionV1(config) => {
            trader_templates::run_okx_swap_copy_target_position_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::OkxSwapCopyTargetPositionBboMaker20260605(config) => {
            trader_templates::run_okx_swap_copy_target_position_bbo_maker_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::OkxSwapTargetLeverageBboPostOnly20260910(config) => {
            let run = trader_templates::run_okx_swap_target_leverage_bbo_post_only_once(
                &config,
                &credential,
            )
            .await
            .map_err(|error| format!("execution failed: {error}"))?;
            database
                .put_okx_swap_target_leverage_state(
                    &trader.id,
                    &OkxSwapTargetLeverageState {
                        account_id: run.snapshot.account_id.clone(),
                        product_id: run.snapshot.product_id.clone(),
                        target_leverage: run.snapshot.target_leverage.normalize().to_string(),
                        target_qty: run.snapshot.target_qty.normalize().to_string(),
                    },
                )
                .map_err(|error| database_error(&error))?;
            return Ok(format!("{run:?}"));
        }
        trader_templates::TraderModel::OkxSwapCopyTargetPositionBboMakerByDirection20260605(config) => {
            trader_templates::run_okx_swap_copy_target_position_bbo_maker_by_direction_once(
                &config,
                &credential,
            )
            .await
            .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::OkxSwapCopyTargetPositionBboMakerByDirectionSingleflight20260609(
            config,
        ) => trader_templates::run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once(
            &config,
            &credential,
        )
        .await
        .map(|run| format!("{run:?}")),
        trader_templates::TraderModel::OkxSwapCopyTargetPositionMultiOrderMaker20260605(config) => {
            trader_templates::run_okx_swap_copy_target_position_multi_order_maker_once(&config, &credential)
                .await
                .map(|run| format!("{run:?}"))
        }
        trader_templates::TraderModel::OkxSwapCopyTargetPositionMultiOrderMakerByDirection20260605(
            config,
        ) => trader_templates::run_okx_swap_copy_target_position_multi_order_maker_by_direction_once(
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
    use super::{templates, validate_configuration};
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

    #[test]
    fn templates_expose_complete_documented_schemas() {
        let templates = templates();
        assert_eq!(templates.len(), 10);
        for template in templates {
            assert!(!template.id.is_empty());
            assert!(!template.name.is_empty());
            assert!(!template.credential_type.is_empty());
            assert!(!template.description.is_empty());
            for schema in [&template.params_schema, &template.signal_schema] {
                assert_eq!(schema["type"], "object");
                assert!(
                    schema["title"]
                        .as_str()
                        .is_some_and(|title| !title.is_empty())
                );
                assert!(
                    schema["description"]
                        .as_str()
                        .is_some_and(|description| !description.is_empty())
                );
                let properties = schema["properties"].as_object().expect("schema properties");
                assert!(!properties.is_empty());
                for property in properties.values() {
                    assert!(
                        property["title"]
                            .as_str()
                            .is_some_and(|title| !title.is_empty())
                    );
                    assert!(
                        property["description"]
                            .as_str()
                            .is_some_and(|description| !description.is_empty())
                    );
                }
            }
        }
    }

    #[test]
    fn template_examples_match_their_execution_configuration() {
        for template in templates() {
            assert!(
                validate_configuration(
                    template.id,
                    "credential-id",
                    &template.params_example,
                    &template.signal_example,
                )
                .is_ok(),
                "{} example configuration is invalid",
                template.id
            );
        }
    }
}
