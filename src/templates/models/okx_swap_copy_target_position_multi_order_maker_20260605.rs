use std::collections::HashMap;
use std::str::FromStr;
use std::sync::OnceLock;

use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxOrderSide, OkxOrderType, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult,
};
use crate::runtime::ResourceClaim;

use super::{Result, TargetPositionAdjustmentSide, TraderRunError};

const OKX_NET_POSITION_MODE: &str = "net_mode";

static INSTRUMENT_CACHE: OnceLock<RwLock<HashMap<String, InstrumentSpec>>> = OnceLock::new();

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapCopyTargetPositionMultiOrderMakerConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub tolerance_qty: Decimal,

    pub order_count: usize,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub max_order_qty: Option<Decimal>,

    #[serde(default)]
    pub broker_code: Option<String>,
}

impl OkxSwapCopyTargetPositionMultiOrderMakerConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapCopyTargetPositionMultiOrderMakerRun {
    pub current_qty: Decimal,
    pub plan: Option<NetMultiOrderPlan>,
    pub cancelled_orders: Vec<String>,
    pub orders: Vec<OkxPlaceOrderResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NetMultiOrderPlan {
    pub side: TargetPositionAdjustmentSide,
    pub target_order_qty: Decimal,
    pub price: Decimal,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct InstrumentSpec {
    lot_size: Decimal,
    min_size: Decimal,
}

/// Checks account-level requirements for this net-position model before a config is stored.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, OKX cannot return account config, or the
/// account is not in net position mode.
pub async fn validate_okx_swap_copy_target_position_multi_order_maker_config(
    credential: &AccountCredential,
) -> Result<()> {
    let client = match credential {
        AccountCredential::OkxApiKeySecretPassphraseV1 {
            api_key,
            api_secret,
            passphrase,
        } => OkxClient::new(OkxCredential {
            api_key: api_key.clone(),
            api_secret: api_secret.clone(),
            passphrase: passphrase.clone(),
        }),
        _ => {
            return Err(TraderRunError::CredentialMismatch);
        }
    };

    validate_okx_swap_copy_target_position_multi_order_maker_config_with_client(&client).await
}

/// Checks account-level requirements with an explicit OKX client.
///
/// # Errors
///
/// Returns an error if OKX cannot return account config, or the account is not in net position mode.
pub async fn validate_okx_swap_copy_target_position_multi_order_maker_config_with_client(
    client: &OkxClient,
) -> Result<()> {
    let account_config = client.get_account_config().await?;
    ensure_okx_success(&account_config.code, &account_config.msg)?;
    let pos_mode = account_config
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing {
            data: "account config",
        })?
        .pos_mode;
    if pos_mode != OKX_NET_POSITION_MODE {
        return Err(TraderRunError::OkxAccountModeMismatch {
            expected: OKX_NET_POSITION_MODE,
            actual: pos_mode,
        });
    }

    Ok(())
}

/// Runs one OKX swap multi-order-maker net target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the OKX response cannot be parsed, the OKX API
/// returns an error code, or an order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_multi_order_maker_once(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionMultiOrderMakerRun> {
    let client = match credential {
        AccountCredential::OkxApiKeySecretPassphraseV1 {
            api_key,
            api_secret,
            passphrase,
        } => OkxClient::new(OkxCredential {
            api_key: api_key.clone(),
            api_secret: api_secret.clone(),
            passphrase: passphrase.clone(),
        }),
        _ => {
            return Err(TraderRunError::CredentialMismatch);
        }
    };

    run_okx_swap_copy_target_position_multi_order_maker_once_with_client(config, &client).await
}

/// Runs one OKX swap multi-order-maker net cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_multi_order_maker_once_with_client(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionMultiOrderMakerRun> {
    let positions = client.get_positions("SWAP", &config.product_id).await?;
    ensure_okx_success(&positions.code, &positions.msg)?;
    let pending_orders = client
        .get_orders_pending("SWAP", &config.product_id)
        .await?;
    ensure_okx_success(&pending_orders.code, &pending_orders.msg)?;
    let ticker = client.get_ticker(&config.product_id).await?;
    ensure_okx_success(&ticker.code, &ticker.msg)?;
    let ticker = ticker
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "ticker" })?;
    let instrument = instrument_spec(client, &config.product_id).await?;

    let current_qty = positions
        .data
        .iter()
        .filter(|position| position.inst_id == config.product_id)
        .map(|position| signed_okx_position_qty(&position.pos_side, &position.pos))
        .try_fold(Decimal::ZERO, |total, position_qty| {
            position_qty.map(|qty| total + qty)
        })?;
    let mut pending_orders = pending_orders
        .data
        .into_iter()
        .filter(|order| order.inst_id == config.product_id)
        .collect::<Vec<_>>();

    let Some(plan) = plan_multi_order_maker(config, current_qty, &ticker.bid_px, &ticker.ask_px)?
    else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(current_qty, None, cancelled_orders, Vec::new()));
    };
    sort_orders_by_price(&mut pending_orders, plan.side)?;

    let order_count = config.order_count.max(2);
    if pending_orders.len() > order_count {
        let cancelled_orders =
            cancel_orders(client, &config.product_id, &pending_orders[order_count..]).await?;
        return Ok(run_result(
            current_qty,
            Some(plan),
            cancelled_orders,
            Vec::new(),
        ));
    }

    let total_order_qty = pending_orders_qty(&pending_orders)?;
    if pending_orders.len() == order_count {
        let farthest_order = &pending_orders[order_count - 1];
        if !same_pending_order_price(farthest_order, &plan)?
            || total_order_qty < plan.target_order_qty
        {
            let cancelled_orders = cancel_orders(
                client,
                &config.product_id,
                std::slice::from_ref(farthest_order),
            )
            .await?;
            return Ok(run_result(
                current_qty,
                Some(plan),
                cancelled_orders,
                Vec::new(),
            ));
        }
        return Ok(run_result(current_qty, Some(plan), Vec::new(), Vec::new()));
    }

    if total_order_qty >= plan.target_order_qty {
        let Some(farthest_order) = pending_orders.last() else {
            return Ok(run_result(current_qty, Some(plan), Vec::new(), Vec::new()));
        };
        if !same_pending_order_price(farthest_order, &plan)? {
            let cancelled_orders = cancel_orders(
                client,
                &config.product_id,
                std::slice::from_ref(farthest_order),
            )
            .await?;
            return Ok(run_result(
                current_qty,
                Some(plan),
                cancelled_orders,
                Vec::new(),
            ));
        }
        return Ok(run_result(current_qty, Some(plan), Vec::new(), Vec::new()));
    }

    let orders_to_create = order_count - pending_orders.len();
    let volume_to_create = plan.target_order_qty - total_order_qty;
    if volume_to_create <= Decimal::ZERO {
        return Ok(run_result(current_qty, Some(plan), Vec::new(), Vec::new()));
    }

    let orders = create_orders(
        client,
        config,
        &plan,
        orders_to_create,
        volume_to_create,
        &instrument,
    )
    .await?;
    Ok(run_result(current_qty, Some(plan), Vec::new(), orders))
}

async fn instrument_spec(client: &OkxClient, product_id: &str) -> Result<InstrumentSpec> {
    let cache = INSTRUMENT_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(spec) = cache.read().await.get(product_id).cloned() {
        return Ok(spec);
    }

    let instruments = client.get_instruments("SWAP", product_id).await?;
    ensure_okx_success(&instruments.code, &instruments.msg)?;
    let instrument = instruments
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "instrument" })?;
    let spec = InstrumentSpec {
        lot_size: Decimal::from_str(&instrument.lot_sz)?,
        min_size: Decimal::from_str(&instrument.min_sz)?,
    };
    cache
        .write()
        .await
        .insert(product_id.to_owned(), spec.clone());
    Ok(spec)
}

fn plan_multi_order_maker(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    current_qty: Decimal,
    bid_px: &str,
    ask_px: &str,
) -> Result<Option<NetMultiOrderPlan>> {
    let delta_qty = config.target_qty - current_qty;
    let abs_delta_qty = delta_qty.abs();
    if abs_delta_qty <= config.tolerance_qty {
        return Ok(None);
    }

    let target_order_qty = match config.max_order_qty {
        Some(max_order_qty) => abs_delta_qty.min(max_order_qty),
        None => abs_delta_qty,
    };
    if target_order_qty <= Decimal::ZERO {
        return Ok(None);
    }

    let side = if delta_qty.is_sign_positive() {
        TargetPositionAdjustmentSide::Buy
    } else {
        TargetPositionAdjustmentSide::Sell
    };
    let price = match side {
        TargetPositionAdjustmentSide::Buy => Decimal::from_str(bid_px)?,
        TargetPositionAdjustmentSide::Sell => Decimal::from_str(ask_px)?,
    };

    Ok(Some(NetMultiOrderPlan {
        side,
        target_order_qty,
        price,
    }))
}

fn sort_orders_by_price(
    orders: &mut [OkxPendingOrder],
    side: TargetPositionAdjustmentSide,
) -> Result<()> {
    for order in orders.iter() {
        Decimal::from_str(&order.px)?;
    }
    orders.sort_by(|left, right| {
        let left_price = Decimal::from_str(&left.px).expect("pending order price was prevalidated");
        let right_price =
            Decimal::from_str(&right.px).expect("pending order price was prevalidated");
        match side {
            TargetPositionAdjustmentSide::Buy => right_price.cmp(&left_price),
            TargetPositionAdjustmentSide::Sell => left_price.cmp(&right_price),
        }
    });
    Ok(())
}

async fn create_orders(
    client: &OkxClient,
    config: &OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    plan: &NetMultiOrderPlan,
    orders_to_create: usize,
    volume_to_create: Decimal,
    instrument: &InstrumentSpec,
) -> Result<Vec<OkxPlaceOrderResult>> {
    let mut orders = Vec::new();
    if volume_to_create < instrument.min_size {
        return Ok(orders);
    }
    let max_valid_orders = (volume_to_create / instrument.lot_size).floor();
    let orders_to_create = orders_to_create.min(max_valid_orders.to_usize().unwrap_or(0));
    if orders_to_create == 0 {
        return Ok(orders);
    }
    let volume_per_order_lots = ((volume_to_create / Decimal::from(orders_to_create))
        / instrument.lot_size)
        .floor()
        .max(Decimal::ONE);
    let volume_per_order = volume_per_order_lots * instrument.lot_size;
    let mut remaining_volume = volume_to_create;
    for order_index in 0..orders_to_create {
        let qty = if order_index + 1 == orders_to_create {
            remaining_volume
        } else {
            volume_per_order
        };
        if qty <= Decimal::ZERO {
            continue;
        }

        let order = post_only_order(&config.product_id, plan, qty, config.broker_code.clone());
        let response = client.post_trade_order(&order).await?;
        orders.push(extract_order_result(
            &response.code,
            &response.msg,
            response.data,
        )?);
        remaining_volume -= qty;
    }
    Ok(orders)
}

fn post_only_order(
    product_id: &str,
    plan: &NetMultiOrderPlan,
    qty: Decimal,
    broker_code: Option<String>,
) -> OkxPlaceOrderRequest {
    OkxPlaceOrderRequest::cross_post_only_limit(
        product_id.to_owned(),
        match plan.side {
            TargetPositionAdjustmentSide::Buy => OkxOrderSide::Buy,
            TargetPositionAdjustmentSide::Sell => OkxOrderSide::Sell,
        },
        qty.normalize().to_string(),
        plan.price.normalize().to_string(),
        None,
        broker_code,
    )
}

async fn cancel_orders(
    client: &OkxClient,
    product_id: &str,
    orders: &[OkxPendingOrder],
) -> Result<Vec<String>> {
    let mut cancelled_orders = Vec::new();
    for order in orders {
        let response = client
            .post_cancel_order(&OkxCancelOrderRequest {
                inst_id: product_id.to_owned(),
                ord_id: order.ord_id.clone(),
            })
            .await?;
        let result = extract_cancel_result(&response.code, &response.msg, response.data)?;
        cancelled_orders.push(result.ord_id);
    }
    Ok(cancelled_orders)
}

fn extract_order_result(
    code: &str,
    message: &str,
    data: Vec<OkxPlaceOrderResult>,
) -> Result<OkxPlaceOrderResult> {
    let order = data.into_iter().next();
    match order {
        Some(order) if order.s_code == "0" => {
            ensure_okx_success(code, message)?;
            Ok(order)
        }
        Some(order) => Err(TraderRunError::OkxOrderRejected {
            code: order.s_code,
            message: order.s_msg,
        }),
        None => {
            ensure_okx_success(code, message)?;
            Err(TraderRunError::OkxOrderMissing)
        }
    }
}

fn extract_cancel_result(
    code: &str,
    message: &str,
    data: Vec<crate::exchanges::okx::OkxCancelOrderResult>,
) -> Result<crate::exchanges::okx::OkxCancelOrderResult> {
    let result = data.into_iter().next();
    match result {
        Some(result) if result.s_code == "0" => {
            ensure_okx_success(code, message)?;
            Ok(result)
        }
        Some(result) => Err(TraderRunError::OkxOrderRejected {
            code: result.s_code,
            message: result.s_msg,
        }),
        None => {
            ensure_okx_success(code, message)?;
            Err(TraderRunError::OkxOrderMissing)
        }
    }
}

fn pending_orders_qty(pending_orders: &[OkxPendingOrder]) -> Result<Decimal> {
    let mut qty = Decimal::ZERO;
    for order in pending_orders {
        qty += Decimal::from_str(&order.sz)?;
    }
    Ok(qty)
}

fn same_pending_order_price(
    pending_order: &OkxPendingOrder,
    plan: &NetMultiOrderPlan,
) -> Result<bool> {
    Ok(pending_order.side == okx_side(plan.side)
        && pending_order.ord_type == OkxOrderType::PostOnly
        && Decimal::from_str(&pending_order.px)? == plan.price)
}

fn okx_side(side: TargetPositionAdjustmentSide) -> OkxOrderSide {
    match side {
        TargetPositionAdjustmentSide::Buy => OkxOrderSide::Buy,
        TargetPositionAdjustmentSide::Sell => OkxOrderSide::Sell,
    }
}

fn signed_okx_position_qty(pos_side: &str, pos: &str) -> Result<Decimal> {
    let qty = Decimal::from_str(pos)?;
    if pos_side == "short" {
        Ok(-qty)
    } else {
        Ok(qty)
    }
}

fn run_result(
    current_qty: Decimal,
    plan: Option<NetMultiOrderPlan>,
    cancelled_orders: Vec<String>,
    orders: Vec<OkxPlaceOrderResult>,
) -> OkxSwapCopyTargetPositionMultiOrderMakerRun {
    OkxSwapCopyTargetPositionMultiOrderMakerRun {
        current_qty,
        plan,
        cancelled_orders,
        orders,
    }
}

fn ensure_okx_success(code: &str, message: &str) -> Result<()> {
    if code == "0" {
        return Ok(());
    }

    Err(TraderRunError::OkxApi {
        code: code.to_owned(),
        message: message.to_owned(),
    })
}
