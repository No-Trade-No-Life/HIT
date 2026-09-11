use std::str::FromStr;

use rust_decimal::Decimal;
use serde::de;
use serde::{Deserialize, Deserializer, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxInstrument, OkxOrderSide, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult, OkxTicker,
};
use crate::runtime::ResourceClaim;

use super::{Result, TargetPositionAdjustment, TargetPositionAdjustmentSide, TraderRunError};

const OKX_NET_POSITION_MODE: &str = "net_mode";

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct OkxSwapQuantizedNetPositionBboPostOnlyConfig {
    pub account_id: String,
    pub product_id: String,
    pub max_abs_signal: i32,
    pub volume_multiplier: i32,
    pub net_position: i32,
    #[serde(default)]
    pub broker_code: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    account_id: String,
    product_id: String,
    max_abs_signal: i32,
    volume_multiplier: i32,
    net_position: i32,
    #[serde(default)]
    broker_code: Option<String>,
}

impl<'de> Deserialize<'de> for OkxSwapQuantizedNetPositionBboPostOnlyConfig {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        if raw.max_abs_signal <= 0 {
            return Err(de::Error::custom("max_abs_signal must be positive"));
        }
        if raw.volume_multiplier <= 0 {
            return Err(de::Error::custom("volume_multiplier must be positive"));
        }
        if i64::from(raw.net_position).abs() > i64::from(raw.max_abs_signal) {
            return Err(de::Error::custom(format!(
                "net_position absolute value must not exceed max_abs_signal ({})",
                raw.max_abs_signal
            )));
        }
        Ok(Self {
            account_id: raw.account_id,
            product_id: raw.product_id,
            max_abs_signal: raw.max_abs_signal,
            volume_multiplier: raw.volume_multiplier,
            net_position: raw.net_position,
            broker_code: raw.broker_code,
        })
    }
}

impl OkxSwapQuantizedNetPositionBboPostOnlyConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }

    #[must_use]
    pub fn target_qty(&self) -> Decimal {
        Decimal::from(self.net_position) * Decimal::from(self.volume_multiplier)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapQuantizedNetPositionBboPostOnlyRun {
    pub current_qty: Decimal,
    pub target_qty: Decimal,
    pub adjustment: Option<TargetPositionAdjustment>,
    pub cancelled_orders: Vec<String>,
    pub order: Option<OkxPlaceOrderResult>,
}

/// Runs one OKX Swap integer net-position BBO post-only cycle.
///
/// `net_position` is an integer signal. The target order quantity is exactly
/// `volume_multiplier * net_position`; the strategy never rounds that value.
///
/// # Errors
///
/// Returns an error if the credential type, account mode, signal configuration, instrument
/// quantity, OKX response, or order response is invalid.
pub async fn run_okx_swap_quantized_net_position_bbo_post_only_once(
    config: &OkxSwapQuantizedNetPositionBboPostOnlyConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapQuantizedNetPositionBboPostOnlyRun> {
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
        _ => return Err(TraderRunError::CredentialMismatch),
    };

    run_okx_swap_quantized_net_position_bbo_post_only_once_with_client(config, &client).await
}

/// Runs one OKX Swap integer net-position BBO post-only cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if an account, position, instrument, ticker, or order response cannot be used.
pub async fn run_okx_swap_quantized_net_position_bbo_post_only_once_with_client(
    config: &OkxSwapQuantizedNetPositionBboPostOnlyConfig,
    client: &OkxClient,
) -> Result<OkxSwapQuantizedNetPositionBboPostOnlyRun> {
    ensure_net_position_mode(client).await?;
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
    let pending_orders = client
        .get_orders_pending("SWAP", &config.product_id)
        .await?;
    ensure_okx_success(&pending_orders.code, &pending_orders.msg)?;
    let pending_orders = pending_orders
        .data
        .into_iter()
        .filter(|order| order.inst_id == config.product_id)
        .collect::<Vec<_>>();
    let adjustment = plan_adjustment(config, current_qty);

    if pending_orders.len() > 1 {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            config.target_qty(),
            adjustment,
            cancelled_orders,
            None,
        ));
    }
    let Some(adjustment) = adjustment else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            config.target_qty(),
            None,
            cancelled_orders,
            None,
        ));
    };

    let ticker = get_ticker(client, &config.product_id).await?;
    let instrument = get_instrument(client, &config.product_id).await?;
    let order = post_only_order(
        &adjustment,
        &ticker,
        &instrument,
        config.broker_code.clone(),
    )?;
    if let Some(pending_order) = pending_orders.first() {
        if same_pending_order(pending_order, &order)? {
            return Ok(run_result(
                current_qty,
                config.target_qty(),
                Some(adjustment),
                Vec::new(),
                None,
            ));
        }
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            config.target_qty(),
            Some(adjustment),
            cancelled_orders,
            None,
        ));
    }

    let response = client.post_trade_order(&order).await?;
    let order = extract_order_result(&response.code, &response.msg, response.data)?;
    Ok(run_result(
        current_qty,
        config.target_qty(),
        Some(adjustment),
        Vec::new(),
        Some(order),
    ))
}

fn plan_adjustment(
    config: &OkxSwapQuantizedNetPositionBboPostOnlyConfig,
    current_qty: Decimal,
) -> Option<TargetPositionAdjustment> {
    let target_qty = config.target_qty();
    let delta_qty = target_qty - current_qty;
    if delta_qty == Decimal::ZERO {
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
        target_qty,
        delta_qty,
        order_qty: delta_qty.abs(),
        side,
    })
}

fn post_only_order(
    adjustment: &TargetPositionAdjustment,
    ticker: &OkxTicker,
    instrument: &OkxInstrument,
    broker_code: Option<String>,
) -> Result<OkxPlaceOrderRequest> {
    ensure_order_quantity(adjustment.order_qty, instrument)?;
    let side = match adjustment.side {
        TargetPositionAdjustmentSide::Buy => OkxOrderSide::Buy,
        TargetPositionAdjustmentSide::Sell => OkxOrderSide::Sell,
    };
    let mut order = OkxPlaceOrderRequest::cross_post_only_limit(
        adjustment.product_id.clone(),
        side,
        adjustment.order_qty.normalize().to_string(),
        bbo_price_for_side(adjustment.side, ticker)?
            .normalize()
            .to_string(),
        None,
        broker_code,
    );
    if reduces_position(adjustment) {
        order.reduce_only = Some("true".to_owned());
    }
    Ok(order)
}

fn ensure_order_quantity(order_qty: Decimal, instrument: &OkxInstrument) -> Result<()> {
    let lot_size = Decimal::from_str(&instrument.lot_sz)?;
    let minimum_size = Decimal::from_str(&instrument.min_sz)?;
    if lot_size <= Decimal::ZERO || minimum_size <= Decimal::ZERO {
        return Err(TraderRunError::OkxQuantizedInvalidLotSize {
            product_id: instrument.inst_id.clone(),
        });
    }
    if order_qty < minimum_size {
        return Err(TraderRunError::OkxQuantizedBelowMinimum {
            product_id: instrument.inst_id.clone(),
            quantity: order_qty,
            minimum_size,
        });
    }
    if (order_qty / lot_size).fract() != Decimal::ZERO {
        return Err(TraderRunError::OkxQuantizedNotLotMultiple {
            product_id: instrument.inst_id.clone(),
            quantity: order_qty,
            lot_size,
        });
    }
    Ok(())
}

fn reduces_position(adjustment: &TargetPositionAdjustment) -> bool {
    adjustment.current_qty != Decimal::ZERO
        && ((adjustment.current_qty.is_sign_positive()
            && adjustment.side == TargetPositionAdjustmentSide::Sell)
            || (adjustment.current_qty.is_sign_negative()
                && adjustment.side == TargetPositionAdjustmentSide::Buy))
}

fn bbo_price_for_side(side: TargetPositionAdjustmentSide, ticker: &OkxTicker) -> Result<Decimal> {
    match side {
        TargetPositionAdjustmentSide::Buy => Decimal::from_str(&ticker.bid_px).map_err(Into::into),
        TargetPositionAdjustmentSide::Sell => Decimal::from_str(&ticker.ask_px).map_err(Into::into),
    }
}

async fn ensure_net_position_mode(client: &OkxClient) -> Result<()> {
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

async fn get_ticker(client: &OkxClient, product_id: &str) -> Result<OkxTicker> {
    let ticker = client.get_ticker(product_id).await?;
    ensure_okx_success(&ticker.code, &ticker.msg)?;
    ticker
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "ticker" })
}

async fn get_instrument(client: &OkxClient, product_id: &str) -> Result<OkxInstrument> {
    let instruments = client.get_instruments("SWAP", product_id).await?;
    ensure_okx_success(&instruments.code, &instruments.msg)?;
    instruments
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "instrument" })
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
        let cancelled = extract_cancel_result(&response.code, &response.msg, response.data)?;
        cancelled_orders.push(cancelled);
    }
    Ok(cancelled_orders)
}

fn extract_order_result(
    code: &str,
    message: &str,
    data: Vec<OkxPlaceOrderResult>,
) -> Result<OkxPlaceOrderResult> {
    match data.into_iter().next() {
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
) -> Result<String> {
    match data.into_iter().next() {
        Some(result) if result.s_code == "0" => {
            ensure_okx_success(code, message)?;
            Ok(result.ord_id)
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

fn same_pending_order(
    pending_order: &OkxPendingOrder,
    target_order: &OkxPlaceOrderRequest,
) -> Result<bool> {
    let Some(target_px) = &target_order.px else {
        return Ok(false);
    };
    let pending_reduce_only = pending_order.reduce_only.as_deref() == Some("true");
    let target_reduce_only = target_order.reduce_only.as_deref() == Some("true");
    Ok(pending_order.side == target_order.side
        && pending_order.ord_type == target_order.ord_type
        && Decimal::from_str(&pending_order.px)? == Decimal::from_str(target_px)?
        && Decimal::from_str(&pending_order.sz)? == Decimal::from_str(&target_order.sz)?
        && pending_reduce_only == target_reduce_only)
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

fn run_result(
    current_qty: Decimal,
    target_qty: Decimal,
    adjustment: Option<TargetPositionAdjustment>,
    cancelled_orders: Vec<String>,
    order: Option<OkxPlaceOrderResult>,
) -> OkxSwapQuantizedNetPositionBboPostOnlyRun {
    OkxSwapQuantizedNetPositionBboPostOnlyRun {
        current_qty,
        target_qty,
        adjustment,
        cancelled_orders,
        order,
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use serde_json::json;

    use super::{
        OkxSwapQuantizedNetPositionBboPostOnlyConfig, ensure_order_quantity, plan_adjustment,
        post_only_order,
    };
    use crate::exchanges::okx::{OkxInstrument, OkxOrderSide, OkxOrderType, OkxTicker};

    fn config(net_position: i32) -> OkxSwapQuantizedNetPositionBboPostOnlyConfig {
        serde_json::from_value(json!({
            "account_id": "credential",
            "product_id": "BTC-USDT-SWAP",
            "max_abs_signal": 3,
            "volume_multiplier": 10,
            "net_position": net_position,
        }))
        .expect("valid quantized config")
    }

    #[test]
    fn target_quantity_is_exact_signal_times_multiplier() {
        assert_eq!(config(-3).target_qty(), Decimal::from(-30));
        assert_eq!(config(0).target_qty(), Decimal::ZERO);
        assert_eq!(config(2).target_qty(), Decimal::from(20));
    }

    #[test]
    fn signal_must_be_integer_and_within_configured_limit() {
        assert!(
            serde_json::from_value::<OkxSwapQuantizedNetPositionBboPostOnlyConfig>(json!({
                "account_id": "credential",
                "product_id": "BTC-USDT-SWAP",
                "max_abs_signal": 3,
                "volume_multiplier": 10,
                "net_position": 4,
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<OkxSwapQuantizedNetPositionBboPostOnlyConfig>(json!({
                "account_id": "credential",
                "product_id": "BTC-USDT-SWAP",
                "max_abs_signal": 3,
                "volume_multiplier": 10,
                "net_position": "2",
            }))
            .is_err()
        );
    }

    #[test]
    fn extra_fields_are_rejected() {
        assert!(
            serde_json::from_value::<OkxSwapQuantizedNetPositionBboPostOnlyConfig>(json!({
                "account_id": "credential",
                "product_id": "BTC-USDT-SWAP",
                "max_abs_signal": 3,
                "volume_multiplier": 10,
                "net_position": 2,
                "target_qty": 2,
            }))
            .is_err()
        );
    }

    #[test]
    fn target_adjustment_uses_signed_contract_quantity() {
        let adjustment = plan_adjustment(&config(-2), Decimal::from(5)).expect("adjustment");
        assert_eq!(adjustment.target_qty, Decimal::from(-20));
        assert_eq!(adjustment.delta_qty, Decimal::from(-25));
        assert_eq!(adjustment.order_qty, Decimal::from(25));
    }

    #[test]
    fn orders_use_side_specific_bbo_and_post_only() {
        let instrument = OkxInstrument {
            inst_id: "BTC-USDT-SWAP".into(),
            lot_sz: "1".into(),
            min_sz: "1".into(),
            ct_val: "0.01".into(),
        };
        let ticker = OkxTicker {
            inst_id: "BTC-USDT-SWAP".into(),
            bid_px: "99".into(),
            ask_px: "101".into(),
        };
        let long = plan_adjustment(&config(2), Decimal::ZERO).expect("long adjustment");
        let long_order = post_only_order(&long, &ticker, &instrument, None).expect("long order");
        assert_eq!(long_order.side, OkxOrderSide::Buy);
        assert_eq!(long_order.ord_type, OkxOrderType::PostOnly);
        assert_eq!(long_order.px.as_deref(), Some("99"));
        assert_eq!(long_order.sz, "20");

        let short = plan_adjustment(&config(-2), Decimal::ZERO).expect("short adjustment");
        let short_order = post_only_order(&short, &ticker, &instrument, None).expect("short order");
        assert_eq!(short_order.side, OkxOrderSide::Sell);
        assert_eq!(short_order.ord_type, OkxOrderType::PostOnly);
        assert_eq!(short_order.px.as_deref(), Some("101"));
        assert_eq!(short_order.sz, "20");
    }

    #[test]
    fn quantity_must_match_okx_lot_and_minimum() {
        let instrument = OkxInstrument {
            inst_id: "BTC-USDT-SWAP".into(),
            lot_sz: "1".into(),
            min_sz: "1".into(),
            ct_val: "0.01".into(),
        };
        assert!(ensure_order_quantity(Decimal::from(10), &instrument).is_ok());
        assert!(
            ensure_order_quantity(Decimal::from_str_exact("0.5").unwrap(), &instrument).is_err()
        );
    }
}
