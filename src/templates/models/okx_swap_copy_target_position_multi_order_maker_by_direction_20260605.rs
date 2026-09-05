use std::cmp::Ordering;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxOrderSide, OkxOrderType, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult, OkxPosition,
};
use crate::runtime::ResourceClaim;

use super::{Result, TraderRunError};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_long_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_short_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub tolerance_qty: Decimal,

    pub order_count: usize,

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

impl OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapCopyTargetPositionMultiOrderMakerByDirectionRun {
    pub long: OkxSwapMultiOrderMakerDirectionRun,
    pub short: OkxSwapMultiOrderMakerDirectionRun,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapMultiOrderMakerDirectionRun {
    pub direction: OkxSwapMultiOrderPositionDirection,
    pub current_qty: Decimal,
    pub target_qty: Decimal,
    pub cancelled_orders: Vec<String>,
    pub orders: Vec<OkxPlaceOrderResult>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OkxSwapMultiOrderPositionDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq)]
struct DirectionTarget {
    direction: OkxSwapMultiOrderPositionDirection,
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
    target_qty: Decimal,
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

struct ProtectedCancelInput<'a> {
    config: &'a OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    target: &'a DirectionTarget,
    state: &'a DirectionState,
    plan: &'a DirectionOrderPlan,
    pending_orders: &'a [OkxPendingOrder],
    bid_px: Decimal,
    ask_px: Decimal,
}

/// Runs one OKX swap multi-order-maker-by-direction target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the OKX response cannot be parsed, the OKX API
/// returns an error code, or an order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_multi_order_maker_by_direction_once(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionMultiOrderMakerByDirectionRun> {
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

    run_okx_swap_copy_target_position_multi_order_maker_by_direction_once_with_client(
        config, &client,
    )
    .await
}

/// Runs one OKX swap multi-order-maker-by-direction cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_multi_order_maker_by_direction_once_with_client(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionMultiOrderMakerByDirectionRun> {
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

    let positions = positions
        .data
        .into_iter()
        .filter(|position| position.inst_id == config.product_id)
        .collect::<Vec<_>>();
    let pending_orders = pending_orders
        .data
        .into_iter()
        .filter(|order| order.inst_id == config.product_id)
        .collect::<Vec<_>>();
    let bid_px = Decimal::from_str(&ticker.bid_px)?;
    let ask_px = Decimal::from_str(&ticker.ask_px)?;

    let long = run_direction(
        client,
        config,
        &positions,
        &pending_orders,
        &DirectionTarget {
            direction: OkxSwapMultiOrderPositionDirection::Long,
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
        &pending_orders,
        &DirectionTarget {
            direction: OkxSwapMultiOrderPositionDirection::Short,
            target_qty: config.target_short_qty,
            target_avg_px: config.target_short_avg_px,
        },
        bid_px,
        ask_px,
    )
    .await?;

    Ok(OkxSwapCopyTargetPositionMultiOrderMakerByDirectionRun { long, short })
}

async fn run_direction(
    client: &OkxClient,
    config: &OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    positions: &[OkxPosition],
    pending_orders: &[OkxPendingOrder],
    target: &DirectionTarget,
    bid_px: Decimal,
    ask_px: Decimal,
) -> Result<OkxSwapMultiOrderMakerDirectionRun> {
    let state = direction_state(positions, target.direction)?;
    let mut pending_orders = pending_orders
        .iter()
        .filter(|order| order.pos_side == pos_side(target.direction))
        .cloned()
        .collect::<Vec<_>>();
    sort_direction_orders(&mut pending_orders, target.direction)?;

    let order_count = config.order_count.max(2);
    if pending_orders.len() > order_count {
        let cancelled_orders =
            cancel_orders(client, &config.product_id, &pending_orders[order_count..]).await?;
        return Ok(direction_run(target, &state, cancelled_orders, Vec::new()));
    }

    let Some(plan) = direction_order_plan(config, target, &state, bid_px, ask_px) else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(direction_run(target, &state, cancelled_orders, Vec::new()));
    };

    let cancelled_orders = cancel_worse_than_protected_price(
        client,
        &ProtectedCancelInput {
            config,
            target,
            state: &state,
            plan: &plan,
            pending_orders: &pending_orders,
            bid_px,
            ask_px,
        },
    )
    .await?;
    if !cancelled_orders.is_empty() {
        return Ok(direction_run(target, &state, cancelled_orders, Vec::new()));
    }

    let total_order_qty = pending_orders_qty(&pending_orders)?;
    if pending_orders.len() == order_count {
        let farthest_order = &pending_orders[order_count - 1];
        if !same_pending_order_price(farthest_order, &plan)? || total_order_qty < plan.target_qty {
            let cancelled_orders = cancel_orders(
                client,
                &config.product_id,
                std::slice::from_ref(farthest_order),
            )
            .await?;
            return Ok(direction_run(target, &state, cancelled_orders, Vec::new()));
        }
        return Ok(direction_run(target, &state, Vec::new(), Vec::new()));
    }

    if !pending_orders.is_empty() && total_order_qty >= plan.target_qty {
        let farthest_order = pending_orders.last().expect("pending_orders is not empty");
        if !same_pending_order_price(farthest_order, &plan)? {
            let cancelled_orders = cancel_orders(
                client,
                &config.product_id,
                std::slice::from_ref(farthest_order),
            )
            .await?;
            return Ok(direction_run(target, &state, cancelled_orders, Vec::new()));
        }
        return Ok(direction_run(target, &state, Vec::new(), Vec::new()));
    }

    let orders_to_create = order_count - pending_orders.len();
    let volume_to_create = plan.target_qty - total_order_qty;
    if volume_to_create <= Decimal::ZERO {
        return Ok(direction_run(target, &state, Vec::new(), Vec::new()));
    }

    let orders = create_orders(
        client,
        &config.product_id,
        &plan,
        orders_to_create,
        volume_to_create,
        config.broker_code.clone(),
    )
    .await?;
    Ok(direction_run(target, &state, Vec::new(), orders))
}

fn direction_order_plan(
    config: &OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
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

    let target_qty = match config.max_order_qty {
        Some(max_order_qty) => abs_delta_qty.min(max_order_qty),
        None => abs_delta_qty,
    };
    if target_qty <= Decimal::ZERO {
        return None;
    }

    let is_open = delta_qty.is_sign_positive();
    let side = match (target.direction, is_open) {
        (OkxSwapMultiOrderPositionDirection::Long, true) => OkxOrderSide::Buy,
        (OkxSwapMultiOrderPositionDirection::Long, false) => OkxOrderSide::Sell,
        (OkxSwapMultiOrderPositionDirection::Short, true) => OkxOrderSide::Sell,
        (OkxSwapMultiOrderPositionDirection::Short, false) => OkxOrderSide::Buy,
    };
    let price = direction_price(&DirectionPriceInput {
        target,
        state,
        side,
        qty: target_qty,
        is_open,
        bid_px,
        ask_px,
        open_slippage: config.open_slippage,
    });

    Some(DirectionOrderPlan {
        side,
        pos_side: pos_side(target.direction).to_owned(),
        target_qty,
        price,
    })
}

async fn cancel_worse_than_protected_price(
    client: &OkxClient,
    input: &ProtectedCancelInput<'_>,
) -> Result<Vec<String>> {
    let Some(open_slippage) = input.config.open_slippage else {
        return Ok(Vec::new());
    };
    let Some(target_avg_px) = input.target.target_avg_px else {
        return Ok(Vec::new());
    };
    let protected_price = protected_price(
        input.target,
        input.state,
        input.plan.target_qty,
        open_slippage,
        target_avg_px,
        input.bid_px,
        input.ask_px,
    );
    let worse_orders = input
        .pending_orders
        .iter()
        .filter(|order| is_worse_order(order, input.plan.side, protected_price).unwrap_or(false))
        .cloned()
        .collect::<Vec<_>>();
    cancel_orders(client, &input.config.product_id, &worse_orders).await
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

    protected_price(
        input.target,
        input.state,
        input.qty,
        open_slippage,
        target_avg_px,
        input.bid_px,
        input.ask_px,
    )
}

fn protected_price(
    target: &DirectionTarget,
    state: &DirectionState,
    qty: Decimal,
    open_slippage: Decimal,
    target_avg_px: Decimal,
    bid_px: Decimal,
    ask_px: Decimal,
) -> Decimal {
    let bbo_price = match target.direction {
        OkxSwapMultiOrderPositionDirection::Long => bid_px,
        OkxSwapMultiOrderPositionDirection::Short => ask_px,
    };
    let direction_sign = match target.direction {
        OkxSwapMultiOrderPositionDirection::Long => Decimal::ONE,
        OkxSwapMultiOrderPositionDirection::Short => -Decimal::ONE,
    };
    let expected_cost =
        target.target_qty * target_avg_px * (Decimal::ONE + direction_sign * open_slippage);
    let actual_cost = state.current_qty * state.current_avg_px;
    let protected_price = (expected_cost - actual_cost) / qty;
    match target.direction {
        OkxSwapMultiOrderPositionDirection::Long => bbo_price.min(protected_price),
        OkxSwapMultiOrderPositionDirection::Short => bbo_price.max(protected_price),
    }
}

fn is_worse_order(
    order: &OkxPendingOrder,
    side: OkxOrderSide,
    protected_price: Decimal,
) -> Result<bool> {
    let price = Decimal::from_str(&order.px)?;
    Ok(match side {
        OkxOrderSide::Buy => price > protected_price,
        OkxOrderSide::Sell => price < protected_price,
    })
}

fn direction_state(
    positions: &[OkxPosition],
    direction: OkxSwapMultiOrderPositionDirection,
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

fn sort_direction_orders(
    orders: &mut [OkxPendingOrder],
    direction: OkxSwapMultiOrderPositionDirection,
) -> Result<()> {
    for order in orders.iter() {
        Decimal::from_str(&order.px)?;
    }
    orders.sort_by(|left, right| {
        let left_price = Decimal::from_str(&left.px).expect("pending order price was prevalidated");
        let right_price =
            Decimal::from_str(&right.px).expect("pending order price was prevalidated");
        match direction {
            OkxSwapMultiOrderPositionDirection::Long => right_price.cmp(&left_price),
            OkxSwapMultiOrderPositionDirection::Short => left_price.cmp(&right_price),
        }
    });
    Ok(())
}

async fn create_orders(
    client: &OkxClient,
    product_id: &str,
    plan: &DirectionOrderPlan,
    orders_to_create: usize,
    volume_to_create: Decimal,
    broker_code: Option<String>,
) -> Result<Vec<OkxPlaceOrderResult>> {
    let mut orders = Vec::new();
    let volume_per_order = volume_to_create / Decimal::from(orders_to_create);
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

        let order = post_only_order(product_id, plan, qty, broker_code.clone());
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
    plan: &DirectionOrderPlan,
    qty: Decimal,
    broker_code: Option<String>,
) -> OkxPlaceOrderRequest {
    OkxPlaceOrderRequest::cross_post_only_limit(
        product_id.to_owned(),
        plan.side,
        qty.normalize().to_string(),
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
    plan: &DirectionOrderPlan,
) -> Result<bool> {
    Ok(pending_order.side == plan.side
        && pending_order.ord_type == OkxOrderType::PostOnly
        && pending_order.pos_side == plan.pos_side
        && Decimal::from_str(&pending_order.px)?.cmp(&plan.price) == Ordering::Equal)
}

fn direction_run(
    target: &DirectionTarget,
    state: &DirectionState,
    cancelled_orders: Vec<String>,
    orders: Vec<OkxPlaceOrderResult>,
) -> OkxSwapMultiOrderMakerDirectionRun {
    OkxSwapMultiOrderMakerDirectionRun {
        direction: target.direction,
        current_qty: state.current_qty,
        target_qty: target.target_qty,
        cancelled_orders,
        orders,
    }
}

fn pos_side(direction: OkxSwapMultiOrderPositionDirection) -> &'static str {
    match direction {
        OkxSwapMultiOrderPositionDirection::Long => "long",
        OkxSwapMultiOrderPositionDirection::Short => "short",
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
