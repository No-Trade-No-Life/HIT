use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::binance_um_futures::{
    BinanceUmFuturesAccountApi, BinanceUmFuturesClient, BinanceUmFuturesCredential,
    BinanceUmFuturesMarketOrderRequest, BinanceUmFuturesOrderResponse, BinanceUmFuturesOrderSide,
    BinanceUmFuturesOrderType, BinanceUmFuturesPositionSide,
};
use crate::runtime::ResourceClaim;

use super::{Result, TargetPositionAdjustment, TraderRunError, plan_target_position_adjustment};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct BinanceUmFuturesCopyTargetPositionConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(default)]
    pub account_api: BinanceUmFuturesAccountApi,

    #[serde(default)]
    pub position_side: Option<BinanceUmFuturesPositionSide>,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_qty: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub tolerance_qty: Decimal,

    #[serde(default, with = "rust_decimal::serde::str_option")]
    pub max_order_qty: Option<Decimal>,
}

impl BinanceUmFuturesCopyTargetPositionConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[must_use]
pub fn plan_binance_um_futures_copy_target_position_adjustment(
    config: &BinanceUmFuturesCopyTargetPositionConfig,
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
pub struct BinanceUmFuturesCopyTargetPositionRun {
    pub current_qty: Decimal,
    pub adjustment: Option<TargetPositionAdjustment>,
    pub order: Option<BinanceUmFuturesOrderResponse>,
}

/// Runs one Binance UM futures target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the exchange response cannot be parsed, or
/// the Binance API call fails.
pub async fn run_binance_um_futures_copy_target_position_once(
    config: &BinanceUmFuturesCopyTargetPositionConfig,
    credential: &AccountCredential,
) -> Result<BinanceUmFuturesCopyTargetPositionRun> {
    let client = match credential {
        AccountCredential::BinanceUmFuturesApiKeySecretV1 {
            api_key,
            api_secret,
        } => BinanceUmFuturesClient::with_account_api(
            BinanceUmFuturesCredential {
                api_key: api_key.clone(),
                api_secret: api_secret.clone(),
            },
            config.account_api,
        ),
        _ => {
            return Err(TraderRunError::CredentialMismatch);
        }
    };

    run_binance_um_futures_copy_target_position_once_with_client(config, &client).await
}

/// Runs one Binance UM futures target-position copy cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the exchange response cannot be parsed or the Binance API call fails.
pub async fn run_binance_um_futures_copy_target_position_once_with_client(
    config: &BinanceUmFuturesCopyTargetPositionConfig,
    client: &BinanceUmFuturesClient,
) -> Result<BinanceUmFuturesCopyTargetPositionRun> {
    let positions = client.get_position_risk(&config.product_id).await?;
    let current_qty = positions
        .iter()
        .filter(|position| position.symbol == config.product_id)
        .filter(|position| {
            config
                .position_side
                .is_none_or(|position_side| position.position_side == position_side.as_str())
        })
        .map(|position| Decimal::from_str(&position.position_amt))
        .try_fold(Decimal::ZERO, |total, position_qty| {
            position_qty.map(|qty| total + qty)
        })?;

    let adjustment = plan_binance_um_futures_copy_target_position_adjustment(config, current_qty);
    let order = match &adjustment {
        Some(adjustment) => Some(
            client
                .post_market_order(&market_order(adjustment, config.position_side))
                .await?,
        ),
        None => None,
    };

    Ok(BinanceUmFuturesCopyTargetPositionRun {
        current_qty,
        adjustment,
        order,
    })
}

fn market_order(
    adjustment: &TargetPositionAdjustment,
    position_side: Option<BinanceUmFuturesPositionSide>,
) -> BinanceUmFuturesMarketOrderRequest {
    BinanceUmFuturesMarketOrderRequest {
        symbol: adjustment.product_id.clone(),
        side: match adjustment.side {
            super::TargetPositionAdjustmentSide::Buy => BinanceUmFuturesOrderSide::BUY,
            super::TargetPositionAdjustmentSide::Sell => BinanceUmFuturesOrderSide::SELL,
        },
        order_type: BinanceUmFuturesOrderType::MARKET,
        quantity: adjustment.order_qty.normalize().to_string(),
        reduce_only: None,
        position_side,
    }
}
