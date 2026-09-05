use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxOrderSide, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult, OkxPosition,
};
use crate::runtime::ResourceClaim;

use super::{Result, TraderRunError};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapCopyTargetPositionBboMakerByDirectionConfig {
    pub account_id: String,
    pub product_id: String,

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

    #[serde(default)]
    pub broker_code: Option<String>,
}

impl OkxSwapCopyTargetPositionBboMakerByDirectionConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

const OKX_LONG_SHORT_POSITION_MODE: &str = "long_short_mode";

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapCopyTargetPositionBboMakerByDirectionRun {
    pub long: OkxSwapBboMakerDirectionRun,
    pub short: OkxSwapBboMakerDirectionRun,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapBboMakerDirectionRun {
    pub direction: OkxSwapPositionDirection,
    pub current_qty: Decimal,
    pub target_qty: Decimal,
    pub cancelled_orders: Vec<String>,
    pub order: Option<OkxPlaceOrderResult>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OkxSwapPositionDirection {
    Long,
    Short,
}

/// Checks account-level requirements for this model before a config is stored.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, OKX cannot return account config, or the
/// account is not in long/short position mode.
pub async fn validate_okx_swap_copy_target_position_bbo_maker_by_direction_config(
    credential: &AccountCredential,
) -> Result<()> {
    let client = okx_client_from_credential(credential)?;

    validate_okx_swap_copy_target_position_bbo_maker_by_direction_config_with_client(&client).await
}

/// Checks account-level requirements with an explicit OKX client.
///
/// # Errors
///
/// Returns an error if OKX cannot return account config, or the account is not in long/short
/// position mode.
pub async fn validate_okx_swap_copy_target_position_bbo_maker_by_direction_config_with_client(
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
    if pos_mode != OKX_LONG_SHORT_POSITION_MODE {
        return Err(TraderRunError::OkxAccountModeMismatch {
            expected: OKX_LONG_SHORT_POSITION_MODE,
            actual: pos_mode,
        });
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionTarget {
    direction: OkxSwapPositionDirection,
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
    side: OkxOrderSide,
    pos_side: String,
    qty: Decimal,
    price: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionPriceInput<'a> {
    target: &'a DirectionTarget,
    state: &'a DirectionState,
    side: OkxOrderSide,
    qty: Decimal,
    is_open: bool,
    bid_px: Decimal,
    ask_px: Decimal,
    open_slippage: Option<Decimal>,
}

/// Runs one OKX swap BBO-maker-by-direction target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the OKX response cannot be parsed, the OKX API
/// returns an error code, or an order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_once(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionRun> {
    let client = okx_client_from_credential(credential)?;

    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client(config, &client).await
}

fn okx_client_from_credential(credential: &AccountCredential) -> Result<OkxClient> {
    let (api_key, api_secret, passphrase, simulated) = match credential {
        AccountCredential::OkxApiKeySecretPassphraseV1 {
            api_key,
            api_secret,
            passphrase,
        } => (api_key, api_secret, passphrase, false),
        AccountCredential::OkxDemoApiKeySecretPassphraseV1 {
            api_key,
            api_secret,
            passphrase,
        } => (api_key, api_secret, passphrase, true),
        _ => return Err(TraderRunError::CredentialMismatch),
    };
    let credential = OkxCredential {
        api_key: api_key.clone(),
        api_secret: api_secret.clone(),
        passphrase: passphrase.clone(),
    };
    Ok(if simulated {
        OkxClient::new_simulated(credential)
    } else {
        OkxClient::new(credential)
    })
}

/// Runs one OKX swap BBO-maker-by-direction cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionRun> {
    let positions = client.get_positions("SWAP", &config.product_id).await?;
    ensure_okx_success(&positions.code, &positions.msg)?;

    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client_and_positions(
        config,
        client,
        positions.data,
    )
    .await
}

/// Runs one OKX swap BBO-maker-by-direction cycle with caller-provided positions.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client_and_positions(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    client: &OkxClient,
    positions: Vec<OkxPosition>,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionRun> {
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

    let positions = positions
        .into_iter()
        .filter(|position| position.inst_id == config.product_id)
        .collect::<Vec<_>>();
    let pending_orders = pending_orders
        .data
        .into_iter()
        .filter(|order| order.inst_id == config.product_id)
        .collect::<Vec<_>>();

    let long = run_direction(
        client,
        config,
        &positions,
        &pending_orders,
        &DirectionTarget {
            direction: OkxSwapPositionDirection::Long,
            target_qty: config.target_long_qty,
            target_avg_px: config.target_long_avg_px,
        },
        Decimal::from_str(&ticker.bid_px)?,
        Decimal::from_str(&ticker.ask_px)?,
    )
    .await?;
    let short = run_direction(
        client,
        config,
        &positions,
        &pending_orders,
        &DirectionTarget {
            direction: OkxSwapPositionDirection::Short,
            target_qty: config.target_short_qty,
            target_avg_px: config.target_short_avg_px,
        },
        Decimal::from_str(&ticker.bid_px)?,
        Decimal::from_str(&ticker.ask_px)?,
    )
    .await?;

    Ok(OkxSwapCopyTargetPositionBboMakerByDirectionRun { long, short })
}

async fn run_direction(
    client: &OkxClient,
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    positions: &[OkxPosition],
    pending_orders: &[OkxPendingOrder],
    target: &DirectionTarget,
    bid_px: Decimal,
    ask_px: Decimal,
) -> Result<OkxSwapBboMakerDirectionRun> {
    let state = direction_state(positions, target.direction)?;
    let pending_orders = pending_orders
        .iter()
        .filter(|order| order.pos_side == pos_side(target.direction))
        .cloned()
        .collect::<Vec<_>>();

    if pending_orders.len() > 1 {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    }

    let Some(plan) = direction_order_plan(config, target, &state, bid_px, ask_px) else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    };
    let order = post_only_order(&config.product_id, &plan, config.broker_code.clone());

    if let Some(pending_order) = pending_orders.first() {
        if same_pending_order(pending_order, &order)? {
            return Ok(direction_run(target, &state, Vec::new(), None));
        }

        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, None));
    }

    let response = client.post_trade_order(&order).await?;
    let order = response.data.into_iter().next();
    let order = match order {
        Some(order) if order.s_code == "0" => {
            ensure_okx_success(&response.code, &response.msg)?;
            Some(order)
        }
        Some(order) => {
            return Err(TraderRunError::OkxOrderRejected {
                code: order.s_code,
                message: order.s_msg,
            });
        }
        None => {
            ensure_okx_success(&response.code, &response.msg)?;
            return Err(TraderRunError::OkxOrderMissing);
        }
    };

    Ok(direction_run(target, &state, Vec::new(), order))
}

fn direction_order_plan(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
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
        (OkxSwapPositionDirection::Long, true) => OkxOrderSide::Buy,
        (OkxSwapPositionDirection::Long, false) => OkxOrderSide::Sell,
        (OkxSwapPositionDirection::Short, true) => OkxOrderSide::Sell,
        (OkxSwapPositionDirection::Short, false) => OkxOrderSide::Buy,
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
        pos_side: pos_side(target.direction).to_owned(),
        qty,
        price,
    })
}

fn direction_price(input: &DirectionPriceInput<'_>) -> Decimal {
    let bbo_price = match input.side {
        OkxOrderSide::Buy => input.bid_px,
        OkxOrderSide::Sell => input.ask_px,
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
        OkxSwapPositionDirection::Long => Decimal::ONE,
        OkxSwapPositionDirection::Short => -Decimal::ONE,
    };
    let expected_cost =
        input.target.target_qty * target_avg_px * (Decimal::ONE + direction_sign * open_slippage);
    let actual_cost = input.state.current_qty * input.state.current_avg_px;
    let protected_price = (expected_cost - actual_cost) / input.qty;
    match input.target.direction {
        OkxSwapPositionDirection::Long => bbo_price.min(protected_price),
        OkxSwapPositionDirection::Short => bbo_price.max(protected_price),
    }
}

fn direction_state(
    positions: &[OkxPosition],
    direction: OkxSwapPositionDirection,
) -> Result<DirectionState> {
    let mut current_qty = Decimal::ZERO;
    let mut position_cost = Decimal::ZERO;
    for position in positions
        .iter()
        .filter(|position| position.pos_side == pos_side(direction))
    {
        let qty = Decimal::from_str(&position.pos)?;
        current_qty += qty;
        let avg_px = if position.avg_px.is_empty() {
            Decimal::ZERO
        } else {
            Decimal::from_str(&position.avg_px)?
        };
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
    broker_code: Option<String>,
) -> OkxPlaceOrderRequest {
    OkxPlaceOrderRequest::cross_post_only_limit(
        product_id.to_owned(),
        plan.side,
        plan.qty.normalize().to_string(),
        plan.price.normalize().to_string(),
        Some(plan.pos_side.clone()),
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
        let result = response.data.into_iter().next();
        match result {
            Some(result) if result.s_code == "0" => {
                ensure_okx_success(&response.code, &response.msg)?;
                cancelled_orders.push(result.ord_id);
            }
            Some(result) => {
                return Err(TraderRunError::OkxOrderRejected {
                    code: result.s_code,
                    message: result.s_msg,
                });
            }
            None => {
                ensure_okx_success(&response.code, &response.msg)?;
                return Err(TraderRunError::OkxOrderMissing);
            }
        }
    }
    Ok(cancelled_orders)
}

fn same_pending_order(
    pending_order: &OkxPendingOrder,
    target_order: &OkxPlaceOrderRequest,
) -> Result<bool> {
    let Some(target_px) = &target_order.px else {
        return Ok(false);
    };
    let Some(target_pos_side) = &target_order.pos_side else {
        return Ok(false);
    };

    Ok(pending_order.side == target_order.side
        && pending_order.ord_type == target_order.ord_type
        && pending_order.pos_side == *target_pos_side
        && Decimal::from_str(&pending_order.px)? == Decimal::from_str(target_px)?
        && Decimal::from_str(&pending_order.sz)? == Decimal::from_str(&target_order.sz)?)
}

fn direction_run(
    target: &DirectionTarget,
    state: &DirectionState,
    cancelled_orders: Vec<String>,
    order: Option<OkxPlaceOrderResult>,
) -> OkxSwapBboMakerDirectionRun {
    OkxSwapBboMakerDirectionRun {
        direction: target.direction,
        current_qty: state.current_qty,
        target_qty: target.target_qty,
        cancelled_orders,
        order,
    }
}

fn pos_side(direction: OkxSwapPositionDirection) -> &'static str {
    match direction {
        OkxSwapPositionDirection::Long => "long",
        OkxSwapPositionDirection::Short => "short",
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
