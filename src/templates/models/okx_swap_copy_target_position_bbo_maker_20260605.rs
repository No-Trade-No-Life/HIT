use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxOrderSide, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult,
};
use crate::runtime::ResourceClaim;

use super::{Result, TargetPositionAdjustment, TargetPositionAdjustmentSide, TraderRunError};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapCopyTargetPositionBboMakerConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub tolerance_qty: Decimal,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub max_order_qty: Option<Decimal>,

    #[serde(default)]
    pub broker_code: Option<String>,
}

impl OkxSwapCopyTargetPositionBboMakerConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapCopyTargetPositionBboMakerRun {
    pub current_qty: Decimal,
    pub adjustment: Option<TargetPositionAdjustment>,
    pub cancelled_orders: Vec<String>,
    pub order: Option<OkxPlaceOrderResult>,
}

/// Runs one OKX swap BBO-maker target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the OKX response cannot be parsed, the OKX API
/// returns an error code, or an order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_once(
    config: &OkxSwapCopyTargetPositionBboMakerConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionBboMakerRun> {
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

    run_okx_swap_copy_target_position_bbo_maker_once_with_client(config, &client).await
}

/// Runs one OKX swap BBO-maker target-position copy cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_once_with_client(
    config: &OkxSwapCopyTargetPositionBboMakerConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionBboMakerRun> {
    let positions = client.get_positions("SWAP", &config.product_id).await?;
    ensure_okx_success(&positions.code, &positions.msg)?;
    let pending_orders = client
        .get_orders_pending("SWAP", &config.product_id)
        .await?;
    ensure_okx_success(&pending_orders.code, &pending_orders.msg)?;

    let current_qty = positions
        .data
        .iter()
        .filter(|position| position.inst_id == config.product_id)
        .map(|position| signed_okx_position_qty(&position.pos_side, &position.pos))
        .try_fold(Decimal::ZERO, |total, position_qty| {
            position_qty.map(|qty| total + qty)
        })?;
    let pending_orders = pending_orders
        .data
        .into_iter()
        .filter(|order| order.inst_id == config.product_id)
        .collect::<Vec<_>>();

    if pending_orders.len() > 1 {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(OkxSwapCopyTargetPositionBboMakerRun {
            current_qty,
            adjustment: None,
            cancelled_orders,
            order: None,
        });
    }

    let adjustment = plan_bbo_maker_adjustment(config, current_qty);
    let Some(adjustment) = adjustment else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(OkxSwapCopyTargetPositionBboMakerRun {
            current_qty,
            adjustment: None,
            cancelled_orders,
            order: None,
        });
    };

    let ticker = client.get_ticker(&config.product_id).await?;
    ensure_okx_success(&ticker.code, &ticker.msg)?;
    let ticker = ticker
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "ticker" })?;
    let order = post_only_order(
        &adjustment,
        &ticker.bid_px,
        &ticker.ask_px,
        config.broker_code.clone(),
    )?;

    let pending_order = pending_orders.first();
    if let Some(pending_order) = pending_order {
        if same_pending_order(pending_order, &order)? {
            return Ok(OkxSwapCopyTargetPositionBboMakerRun {
                current_qty,
                adjustment: Some(adjustment),
                cancelled_orders: Vec::new(),
                order: None,
            });
        }

        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(OkxSwapCopyTargetPositionBboMakerRun {
            current_qty,
            adjustment: Some(adjustment),
            cancelled_orders,
            order: None,
        });
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

    Ok(OkxSwapCopyTargetPositionBboMakerRun {
        current_qty,
        adjustment: Some(adjustment),
        cancelled_orders: Vec::new(),
        order,
    })
}

fn plan_bbo_maker_adjustment(
    config: &OkxSwapCopyTargetPositionBboMakerConfig,
    current_qty: Decimal,
) -> Option<TargetPositionAdjustment> {
    let delta_qty = config.target_qty - current_qty;
    let abs_delta_qty = delta_qty.abs();
    if abs_delta_qty <= config.tolerance_qty {
        return None;
    }

    let order_qty = match config.max_order_qty {
        Some(max_order_qty) => abs_delta_qty.min(max_order_qty),
        None => abs_delta_qty,
    };
    if order_qty <= Decimal::ZERO {
        return None;
    }

    let side = if delta_qty.is_sign_positive() {
        TargetPositionAdjustmentSide::Buy
    } else {
        TargetPositionAdjustmentSide::Sell
    };

    Some(TargetPositionAdjustment {
        account_id: config.account_id.clone(),
        product_id: config.product_id.clone(),
        current_qty,
        target_qty: config.target_qty,
        delta_qty,
        order_qty,
        side,
    })
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

fn post_only_order(
    adjustment: &TargetPositionAdjustment,
    bid_px: &str,
    ask_px: &str,
    broker_code: Option<String>,
) -> Result<OkxPlaceOrderRequest> {
    Ok(OkxPlaceOrderRequest::cross_post_only_limit(
        adjustment.product_id.clone(),
        match adjustment.side {
            TargetPositionAdjustmentSide::Buy => OkxOrderSide::Buy,
            TargetPositionAdjustmentSide::Sell => OkxOrderSide::Sell,
        },
        adjustment.order_qty.normalize().to_string(),
        match adjustment.side {
            TargetPositionAdjustmentSide::Buy => Decimal::from_str(bid_px)?,
            TargetPositionAdjustmentSide::Sell => Decimal::from_str(ask_px)?,
        }
        .normalize()
        .to_string(),
        None,
        broker_code,
    ))
}

fn same_pending_order(
    pending_order: &OkxPendingOrder,
    target_order: &OkxPlaceOrderRequest,
) -> Result<bool> {
    let Some(target_px) = &target_order.px else {
        return Ok(false);
    };

    Ok(pending_order.side == target_order.side
        && pending_order.ord_type == target_order.ord_type
        && Decimal::from_str(&pending_order.px)? == Decimal::from_str(target_px)?
        && Decimal::from_str(&pending_order.sz)? == Decimal::from_str(&target_order.sz)?)
}

fn signed_okx_position_qty(pos_side: &str, pos: &str) -> Result<Decimal> {
    let qty = Decimal::from_str(pos)?;
    if pos_side == "short" {
        Ok(-qty)
    } else {
        Ok(qty)
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
