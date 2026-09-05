use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxClient, OkxCredential, OkxOrderSide, OkxOrderType, OkxPlaceOrderRequest, OkxPlaceOrderResult,
};
use crate::runtime::ResourceClaim;

use super::{
    Result, TargetPositionAdjustment, TargetPositionAdjustmentSide, TraderRunError,
    plan_target_position_adjustment,
};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapCopyTargetPositionConfig {
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

impl OkxSwapCopyTargetPositionConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[must_use]
pub fn plan_okx_swap_copy_target_position_adjustment(
    config: &OkxSwapCopyTargetPositionConfig,
    current_qty: Decimal,
) -> Option<TargetPositionAdjustment> {
    plan_target_position_adjustment(
        &config.account_id,
        &config.product_id,
        config.target_qty,
        config.tolerance_qty,
        config.max_order_qty,
        current_qty,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapCopyTargetPositionRun {
    pub current_qty: Decimal,
    pub adjustment: Option<TargetPositionAdjustment>,
    pub order: Option<OkxPlaceOrderResult>,
}

/// Runs one OKX swap target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the exchange response cannot be parsed, the
/// OKX API returns an error code, or the submitted order is rejected.
pub async fn run_okx_swap_copy_target_position_once(
    config: &OkxSwapCopyTargetPositionConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionRun> {
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

    run_okx_swap_copy_target_position_once_with_client(config, &client).await
}

/// Runs one OKX swap target-position copy cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the exchange response cannot be parsed, the OKX API returns an error code,
/// or the submitted order is rejected.
pub async fn run_okx_swap_copy_target_position_once_with_client(
    config: &OkxSwapCopyTargetPositionConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionRun> {
    let positions = client.get_positions("SWAP", &config.product_id).await?;
    ensure_okx_success(&positions.code, &positions.msg)?;

    let current_qty = positions
        .data
        .iter()
        .filter(|position| position.inst_id == config.product_id)
        .map(|position| signed_okx_position_qty(&position.pos_side, &position.pos))
        .try_fold(Decimal::ZERO, |total, position_qty| {
            position_qty.map(|qty| total + qty)
        })?;

    let adjustment = plan_okx_swap_copy_target_position_adjustment(config, current_qty);
    let order = match &adjustment {
        Some(adjustment) => {
            let response = client
                .post_trade_order(&market_order(adjustment, config.broker_code.clone()))
                .await?;
            let order = response.data.into_iter().next();
            match order {
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
            }
        }
        None => None,
    };

    Ok(OkxSwapCopyTargetPositionRun {
        current_qty,
        adjustment,
        order,
    })
}

fn market_order(
    adjustment: &TargetPositionAdjustment,
    broker_code: Option<String>,
) -> OkxPlaceOrderRequest {
    OkxPlaceOrderRequest {
        inst_id: adjustment.product_id.clone(),
        td_mode: "cross".to_owned(),
        side: match adjustment.side {
            TargetPositionAdjustmentSide::Buy => OkxOrderSide::Buy,
            TargetPositionAdjustmentSide::Sell => OkxOrderSide::Sell,
        },
        ord_type: OkxOrderType::Market,
        sz: adjustment.order_qty.normalize().to_string(),
        px: None,
        pos_side: None,
        reduce_only: None,
        tag: broker_code,
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

fn ensure_okx_success(code: &str, message: &str) -> Result<()> {
    if code == "0" {
        return Ok(());
    }

    Err(TraderRunError::OkxApi {
        code: code.to_owned(),
        message: message.to_owned(),
    })
}
