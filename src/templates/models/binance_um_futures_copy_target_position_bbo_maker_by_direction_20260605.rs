use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::binance_um_futures::{
    BinanceUmFuturesAccountApi, BinanceUmFuturesClient, BinanceUmFuturesCredential,
    BinanceUmFuturesLimitOrderRequest, BinanceUmFuturesOpenOrder, BinanceUmFuturesOrderResponse,
    BinanceUmFuturesOrderSide, BinanceUmFuturesOrderType, BinanceUmFuturesPositionRisk,
    BinanceUmFuturesTimeInForce,
};
use crate::runtime::ResourceClaim;

use super::{Result, TraderRunError};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(default)]
    pub account_api: BinanceUmFuturesAccountApi,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_long_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_short_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub tolerance_qty: Decimal,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub max_order_qty: Option<Decimal>,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub open_slippage: Option<Decimal>,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub target_long_avg_px: Option<Decimal>,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub target_short_avg_px: Option<Decimal>,
}

impl BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceUmFuturesCopyTargetPositionBboMakerByDirectionRun {
    pub long: BinanceUmFuturesBboMakerDirectionRun,
    pub short: BinanceUmFuturesBboMakerDirectionRun,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinanceUmFuturesBboMakerDirectionRun {
    pub direction: BinanceUmFuturesPositionDirection,
    pub current_qty: Decimal,
    pub target_qty: Decimal,
    pub cancelled_orders: Vec<u64>,
    pub order: Option<BinanceUmFuturesOrderResponse>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BinanceUmFuturesPositionDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionTarget {
    direction: BinanceUmFuturesPositionDirection,
    target_qty: Decimal,
    target_avg_px: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionState {
    current_qty: Decimal,
    current_avg_px: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionOrderPlan {
    side: BinanceUmFuturesOrderSide,
    position_side: String,
    qty: Decimal,
    price: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionPriceInput<'a> {
    target: &'a DirectionTarget,
    state: &'a DirectionState,
    side: BinanceUmFuturesOrderSide,
    qty: Decimal,
    is_open: bool,
    bid_px: Decimal,
    ask_px: Decimal,
    open_slippage: Option<Decimal>,
}

/// Checks account-level requirements for this model before a config is stored.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, Binance cannot return the position mode, or
/// the account is not in Hedge Mode.
pub async fn validate_binance_um_futures_copy_target_position_bbo_maker_by_direction_config(
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    credential: &AccountCredential,
) -> Result<()> {
    let client = client_from_credential(config, credential)?;

    validate_binance_um_futures_copy_target_position_bbo_maker_by_direction_config_with_client(
        &client,
    )
    .await
}

/// Checks account-level requirements with an explicit Binance UM Futures client.
///
/// # Errors
///
/// Returns an error if Binance cannot return the position mode, or the account is not in Hedge
/// Mode.
pub async fn validate_binance_um_futures_copy_target_position_bbo_maker_by_direction_config_with_client(
    client: &BinanceUmFuturesClient,
) -> Result<()> {
    let position_mode = client.get_position_side_dual().await?;
    if !position_mode.dual_side_position {
        return Err(TraderRunError::BinanceUmFuturesAccountModeMismatch);
    }

    Ok(())
}

/// Runs one Binance UM Futures BBO-maker-by-direction target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the exchange response cannot be parsed, or a
/// Binance API call fails.
pub async fn run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once(
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    credential: &AccountCredential,
) -> Result<BinanceUmFuturesCopyTargetPositionBboMakerByDirectionRun> {
    let client = client_from_credential(config, credential)?;

    run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once_with_client(
        config, &client,
    )
    .await
}

fn client_from_credential(
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    credential: &AccountCredential,
) -> Result<BinanceUmFuturesClient> {
    match credential {
        AccountCredential::BinanceUmFuturesApiKeySecretV1 {
            api_key,
            api_secret,
        } => Ok(BinanceUmFuturesClient::with_account_api(
            BinanceUmFuturesCredential {
                api_key: api_key.clone(),
                api_secret: api_secret.clone(),
            },
            config.account_api,
        )),
        _ => Err(TraderRunError::CredentialMismatch),
    }
}

/// Runs one Binance UM Futures BBO-maker-by-direction cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the exchange response cannot be parsed or a Binance API call fails.
pub async fn run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once_with_client(
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    client: &BinanceUmFuturesClient,
) -> Result<BinanceUmFuturesCopyTargetPositionBboMakerByDirectionRun> {
    let positions = client.get_position_risk(&config.product_id).await?;
    let open_orders = client.get_open_orders(&config.product_id).await?;
    let ticker = client.get_book_ticker(&config.product_id).await?;
    let bid_px = Decimal::from_str(&ticker.bid_price)?;
    let ask_px = Decimal::from_str(&ticker.ask_price)?;
    let positions = positions
        .into_iter()
        .filter(|position| position.symbol == config.product_id)
        .collect::<Vec<_>>();
    let open_orders = open_orders
        .into_iter()
        .filter(|order| order.symbol == config.product_id)
        .collect::<Vec<_>>();

    let long = run_direction(
        client,
        config,
        &positions,
        &open_orders,
        &DirectionTarget {
            direction: BinanceUmFuturesPositionDirection::Long,
            target_qty: config.target_long_qty,
            target_avg_px: config.target_long_avg_px,
        },
        bid_px,
        ask_px,
    )
    .await?;
    let short = run_direction(
        client,
        config,
        &positions,
        &open_orders,
        &DirectionTarget {
            direction: BinanceUmFuturesPositionDirection::Short,
            target_qty: config.target_short_qty,
            target_avg_px: config.target_short_avg_px,
        },
        bid_px,
        ask_px,
    )
    .await?;

    Ok(BinanceUmFuturesCopyTargetPositionBboMakerByDirectionRun { long, short })
}

async fn run_direction(
    client: &BinanceUmFuturesClient,
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    positions: &[BinanceUmFuturesPositionRisk],
    open_orders: &[BinanceUmFuturesOpenOrder],
    target: &DirectionTarget,
    bid_px: Decimal,
    ask_px: Decimal,
) -> Result<BinanceUmFuturesBboMakerDirectionRun> {
    let state = direction_state(positions, target.direction)?;
    let open_orders = open_orders
        .iter()
        .filter(|order| order.position_side == position_side(target.direction))
        .cloned()
        .collect::<Vec<_>>();

    if open_orders.len() > 1 {
        let cancelled_orders = cancel_orders(client, &config.product_id, &open_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    }

    let Some(plan) = direction_order_plan(config, target, &state, bid_px, ask_px) else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &open_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    };
    let order = post_only_order(&config.product_id, &plan);

    if let Some(open_order) = open_orders.first() {
        if same_open_order(open_order, &order)? {
            return Ok(direction_run(target, &state, Vec::new(), None));
        }

        let cancelled_orders = cancel_orders(client, &config.product_id, &open_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    }

    let order = client.post_limit_order(&order).await?;
    Ok(direction_run(target, &state, Vec::new(), Some(order)))
}

fn direction_order_plan(
    config: &BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    target: &DirectionTarget,
    state: &DirectionState,
    bid_px: Decimal,
    ask_px: Decimal,
) -> Option<DirectionOrderPlan> {
    let delta_qty = target.target_qty - state.current_qty;
    let abs_delta_qty = delta_qty.abs();
    if abs_delta_qty <= config.tolerance_qty {
        return None;
    }

    let qty = match config.max_order_qty {
        Some(max_order_qty) => abs_delta_qty.min(max_order_qty),
        None => abs_delta_qty,
    };
    if qty <= Decimal::ZERO {
        return None;
    }

    let is_open = delta_qty.is_sign_positive();
    let side = match (target.direction, is_open) {
        (BinanceUmFuturesPositionDirection::Long, true) => BinanceUmFuturesOrderSide::BUY,
        (BinanceUmFuturesPositionDirection::Long, false) => BinanceUmFuturesOrderSide::SELL,
        (BinanceUmFuturesPositionDirection::Short, true) => BinanceUmFuturesOrderSide::SELL,
        (BinanceUmFuturesPositionDirection::Short, false) => BinanceUmFuturesOrderSide::BUY,
    };
    let price = direction_price(&DirectionPriceInput {
        target,
        state,
        side,
        qty,
        is_open,
        bid_px,
        ask_px,
        open_slippage: config.open_slippage,
    });

    Some(DirectionOrderPlan {
        side,
        position_side: position_side(target.direction).to_owned(),
        qty,
        price,
    })
}

fn direction_price(input: &DirectionPriceInput<'_>) -> Decimal {
    let bbo_price = match input.side {
        BinanceUmFuturesOrderSide::BUY => input.bid_px,
        BinanceUmFuturesOrderSide::SELL => input.ask_px,
    };
    let Some(open_slippage) = input.open_slippage else {
        return bbo_price;
    };
    if !input.is_open {
        return bbo_price;
    }
    let Some(target_avg_px) = input.target.target_avg_px else {
        return bbo_price;
    };

    let direction_sign = match input.target.direction {
        BinanceUmFuturesPositionDirection::Long => Decimal::ONE,
        BinanceUmFuturesPositionDirection::Short => -Decimal::ONE,
    };
    let expected_cost =
        input.target.target_qty * target_avg_px * (Decimal::ONE + direction_sign * open_slippage);
    let actual_cost = input.state.current_qty * input.state.current_avg_px;
    let protected_price = (expected_cost - actual_cost) / input.qty;
    match input.target.direction {
        BinanceUmFuturesPositionDirection::Long => bbo_price.min(protected_price),
        BinanceUmFuturesPositionDirection::Short => bbo_price.max(protected_price),
    }
}

fn direction_state(
    positions: &[BinanceUmFuturesPositionRisk],
    direction: BinanceUmFuturesPositionDirection,
) -> Result<DirectionState> {
    let mut current_qty = Decimal::ZERO;
    let mut position_cost = Decimal::ZERO;
    for position in positions
        .iter()
        .filter(|position| position.position_side == position_side(direction))
    {
        let qty = Decimal::from_str(&position.position_amt)?.abs();
        current_qty += qty;
        let avg_px = Decimal::from_str(&position.entry_price)?;
        position_cost += qty * avg_px;
    }

    Ok(DirectionState {
        current_qty,
        current_avg_px: if current_qty == Decimal::ZERO {
            Decimal::ZERO
        } else {
            position_cost / current_qty
        },
    })
}

fn post_only_order(
    product_id: &str,
    plan: &DirectionOrderPlan,
) -> BinanceUmFuturesLimitOrderRequest {
    BinanceUmFuturesLimitOrderRequest {
        symbol: product_id.to_owned(),
        side: plan.side,
        order_type: BinanceUmFuturesOrderType::LIMIT,
        time_in_force: BinanceUmFuturesTimeInForce::GTX,
        quantity: plan.qty.normalize().to_string(),
        price: plan.price.normalize().to_string(),
        reduce_only: None,
        position_side: Some(plan.position_side.clone()),
    }
}

async fn cancel_orders(
    client: &BinanceUmFuturesClient,
    product_id: &str,
    orders: &[BinanceUmFuturesOpenOrder],
) -> Result<Vec<u64>> {
    let mut cancelled_orders = Vec::new();
    for order in orders {
        let response = client.delete_order(product_id, order.order_id).await?;
        cancelled_orders.push(response.order_id);
    }
    Ok(cancelled_orders)
}

fn same_open_order(
    open_order: &BinanceUmFuturesOpenOrder,
    target_order: &BinanceUmFuturesLimitOrderRequest,
) -> Result<bool> {
    let Some(target_position_side) = &target_order.position_side else {
        return Ok(false);
    };

    Ok(open_order.side == target_order.side
        && open_order.order_type == target_order.order_type
        && open_order.time_in_force == "GTX"
        && open_order.position_side == *target_position_side
        && Decimal::from_str(&open_order.price)? == Decimal::from_str(&target_order.price)?
        && Decimal::from_str(&open_order.orig_qty)? == Decimal::from_str(&target_order.quantity)?)
}

fn direction_run(
    target: &DirectionTarget,
    state: &DirectionState,
    cancelled_orders: Vec<u64>,
    order: Option<BinanceUmFuturesOrderResponse>,
) -> BinanceUmFuturesBboMakerDirectionRun {
    BinanceUmFuturesBboMakerDirectionRun {
        direction: target.direction,
        current_qty: state.current_qty,
        target_qty: target.target_qty,
        cancelled_orders,
        order,
    }
}

fn position_side(direction: BinanceUmFuturesPositionDirection) -> &'static str {
    match direction {
        BinanceUmFuturesPositionDirection::Long => "LONG",
        BinanceUmFuturesPositionDirection::Short => "SHORT",
    }
}
