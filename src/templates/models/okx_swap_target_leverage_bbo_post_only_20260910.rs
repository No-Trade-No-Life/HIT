use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::AccountCredential;
use crate::exchanges::okx::{
    OkxCancelOrderRequest, OkxClient, OkxCredential, OkxInstrument, OkxOrderSide, OkxPendingOrder,
    OkxPlaceOrderRequest, OkxPlaceOrderResult, OkxTicker,
};
use crate::runtime::ResourceClaim;

use super::{Result, TargetPositionAdjustment, TargetPositionAdjustmentSide, TraderRunError};

const OKX_NET_POSITION_MODE: &str = "net_mode";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapTargetLeverageBboPostOnlyConfig {
    pub account_id: String,
    pub product_id: String,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_leverage: Decimal,

    #[serde(default)]
    pub broker_code: Option<String>,

    #[serde(default)]
    pub snapshot: Option<OkxSwapTargetLeverageSnapshot>,
}

impl OkxSwapTargetLeverageBboPostOnlyConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.product_id),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct OkxSwapTargetLeverageSnapshot {
    pub account_id: String,
    pub product_id: String,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_leverage: Decimal,

    #[serde(with = "rust_decimal::serde::str")]
    pub target_qty: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OkxSwapTargetLeverageBboPostOnlyRun {
    pub current_qty: Decimal,
    pub snapshot: OkxSwapTargetLeverageSnapshot,
    pub adjustment: Option<TargetPositionAdjustment>,
    pub cancelled_orders: Vec<String>,
    pub order: Option<OkxPlaceOrderResult>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum SnapshotDecision {
    Use(OkxSwapTargetLeverageSnapshot),
    Calculate,
    Flatten,
}

/// Runs one OKX swap BBO post-only target-leverage cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the account is not in net position mode, an
/// OKX response is invalid, or an order/cancel request is rejected.
pub async fn run_okx_swap_target_leverage_bbo_post_only_once(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapTargetLeverageBboPostOnlyRun> {
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

    run_okx_swap_target_leverage_bbo_post_only_once_with_client(config, &client).await
}

/// Runs one OKX swap BBO post-only target-leverage cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the account configuration, position, balance, instrument, ticker, or order
/// response cannot be used to execute the target-leverage cycle.
pub async fn run_okx_swap_target_leverage_bbo_post_only_once_with_client(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
    client: &OkxClient,
) -> Result<OkxSwapTargetLeverageBboPostOnlyRun> {
    ensure_net_position_mode(client).await?;
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

    let decision = snapshot_decision(config, current_qty, !pending_orders.is_empty());
    let (snapshot, calculated_ticker) = match decision {
        SnapshotDecision::Use(snapshot) => (snapshot, None),
        SnapshotDecision::Flatten => (snapshot_for_flatten(config), None),
        SnapshotDecision::Calculate => {
            let (snapshot, ticker) = calculate_snapshot(config, client).await?;
            (snapshot, Some(ticker))
        }
    };
    if pending_orders.len() > 1 {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            snapshot,
            None,
            cancelled_orders,
            None,
        ));
    }
    let adjustment = plan_adjustment(config, current_qty, snapshot.target_qty);
    let Some(adjustment) = adjustment else {
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            snapshot,
            None,
            cancelled_orders,
            None,
        ));
    };

    let ticker = match calculated_ticker {
        Some(ticker) => ticker,
        None => get_ticker(client, &config.product_id).await?,
    };
    let order = post_only_order(&adjustment, &ticker, config.broker_code.clone())?;
    if let Some(pending_order) = pending_orders.first() {
        if same_pending_order(pending_order, &order)? {
            return Ok(run_result(
                current_qty,
                snapshot,
                Some(adjustment),
                Vec::new(),
                None,
            ));
        }
        let cancelled_orders = cancel_orders(client, &config.product_id, &pending_orders).await?;
        return Ok(run_result(
            current_qty,
            snapshot,
            Some(adjustment),
            cancelled_orders,
            None,
        ));
    }

    let response = client.post_trade_order(&order).await?;
    let order = extract_order_result(&response.code, &response.msg, response.data)?;
    Ok(run_result(
        current_qty,
        snapshot,
        Some(adjustment),
        Vec::new(),
        Some(order),
    ))
}

fn snapshot_decision(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
    current_qty: Decimal,
    has_pending_order: bool,
) -> SnapshotDecision {
    if config.target_leverage == Decimal::ZERO {
        return SnapshotDecision::Flatten;
    }
    if current_qty * config.target_leverage < Decimal::ZERO {
        return SnapshotDecision::Flatten;
    }
    match &config.snapshot {
        Some(snapshot)
            if snapshot.account_id == config.account_id
                && snapshot.product_id == config.product_id
                && snapshot.target_leverage == config.target_leverage =>
        {
            if current_qty != Decimal::ZERO
                || (has_pending_order && snapshot.target_qty != Decimal::ZERO)
            {
                SnapshotDecision::Use(snapshot.clone())
            } else {
                SnapshotDecision::Calculate
            }
        }
        _ => SnapshotDecision::Calculate,
    }
}

async fn calculate_snapshot(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
    client: &OkxClient,
) -> Result<(OkxSwapTargetLeverageSnapshot, OkxTicker)> {
    let account_balance = client.get_account_balance().await?;
    ensure_okx_success(&account_balance.code, &account_balance.msg)?;
    let equity = account_balance
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing {
            data: "account balance",
        })?
        .total_eq;
    let equity = Decimal::from_str(&equity)?;
    if equity <= Decimal::ZERO {
        return Err(TraderRunError::OkxTargetLeverageNonPositiveEquity { equity });
    }
    let ticker = get_ticker(client, &config.product_id).await?;
    let instruments = client.get_instruments("SWAP", &config.product_id).await?;
    ensure_okx_success(&instruments.code, &instruments.msg)?;
    let instrument = instruments
        .data
        .into_iter()
        .next()
        .ok_or(TraderRunError::OkxDataMissing { data: "instrument" })?;
    let target_qty = target_qty(
        config.target_leverage,
        equity,
        &instrument,
        bbo_price(config.target_leverage, &ticker)?,
    )?;
    Ok((
        OkxSwapTargetLeverageSnapshot {
            account_id: config.account_id.clone(),
            product_id: config.product_id.clone(),
            target_leverage: config.target_leverage,
            target_qty,
        },
        ticker,
    ))
}

fn target_qty(
    target_leverage: Decimal,
    equity: Decimal,
    instrument: &OkxInstrument,
    price: Decimal,
) -> Result<Decimal> {
    if price <= Decimal::ZERO {
        return Err(TraderRunError::OkxTargetLeverageInvalidBbo {
            product_id: instrument.inst_id.clone(),
            price,
        });
    }
    let contract_value = Decimal::from_str(&instrument.ct_val)?;
    if contract_value <= Decimal::ZERO {
        return Err(TraderRunError::OkxTargetLeverageInvalidContractValue {
            product_id: instrument.inst_id.clone(),
            contract_value,
        });
    }
    let lot_size = Decimal::from_str(&instrument.lot_sz)?;
    let minimum_size = Decimal::from_str(&instrument.min_sz)?;
    if lot_size <= Decimal::ZERO || minimum_size <= Decimal::ZERO {
        return Err(TraderRunError::OkxTargetLeverageInvalidLotSize {
            product_id: instrument.inst_id.clone(),
        });
    }
    let quantity =
        ((target_leverage.abs() * equity / (contract_value * price)) / lot_size).floor() * lot_size;
    if quantity < minimum_size {
        return Err(TraderRunError::OkxTargetLeverageBelowMinimum {
            product_id: instrument.inst_id.clone(),
            quantity,
            minimum_size,
        });
    }
    Ok(if target_leverage.is_sign_negative() {
        -quantity
    } else {
        quantity
    })
}

fn snapshot_for_flatten(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
) -> OkxSwapTargetLeverageSnapshot {
    OkxSwapTargetLeverageSnapshot {
        account_id: config.account_id.clone(),
        product_id: config.product_id.clone(),
        target_leverage: config.target_leverage,
        target_qty: Decimal::ZERO,
    }
}

fn plan_adjustment(
    config: &OkxSwapTargetLeverageBboPostOnlyConfig,
    current_qty: Decimal,
    target_qty: Decimal,
) -> Option<TargetPositionAdjustment> {
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
    broker_code: Option<String>,
) -> Result<OkxPlaceOrderRequest> {
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

fn reduces_position(adjustment: &TargetPositionAdjustment) -> bool {
    adjustment.current_qty != Decimal::ZERO
        && ((adjustment.current_qty.is_sign_positive()
            && adjustment.side == TargetPositionAdjustmentSide::Sell)
            || (adjustment.current_qty.is_sign_negative()
                && adjustment.side == TargetPositionAdjustmentSide::Buy))
}

fn bbo_price(target_leverage: Decimal, ticker: &OkxTicker) -> Result<Decimal> {
    if target_leverage.is_sign_negative() {
        Decimal::from_str(&ticker.ask_px).map_err(Into::into)
    } else {
        Decimal::from_str(&ticker.bid_px).map_err(Into::into)
    }
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
    snapshot: OkxSwapTargetLeverageSnapshot,
    adjustment: Option<TargetPositionAdjustment>,
    cancelled_orders: Vec<String>,
    order: Option<OkxPlaceOrderResult>,
) -> OkxSwapTargetLeverageBboPostOnlyRun {
    OkxSwapTargetLeverageBboPostOnlyRun {
        current_qty,
        snapshot,
        adjustment,
        cancelled_orders,
        order,
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{
        OkxSwapTargetLeverageBboPostOnlyConfig, OkxSwapTargetLeverageSnapshot, SnapshotDecision,
        TargetPositionAdjustment, TargetPositionAdjustmentSide, bbo_price, plan_adjustment,
        post_only_order, snapshot_decision, target_qty,
    };
    use crate::exchanges::okx::{OkxInstrument, OkxOrderSide, OkxOrderType, OkxTicker};

    fn decimal(value: i64) -> Decimal {
        Decimal::from(value)
    }

    fn config(
        target_leverage: i64,
        snapshot: Option<OkxSwapTargetLeverageSnapshot>,
    ) -> OkxSwapTargetLeverageBboPostOnlyConfig {
        OkxSwapTargetLeverageBboPostOnlyConfig {
            account_id: "credential".to_owned(),
            product_id: "BTC-USDT-SWAP".to_owned(),
            target_leverage: decimal(target_leverage),
            broker_code: None,
            snapshot,
        }
    }

    fn snapshot(target_leverage: i64, target_qty: i64) -> OkxSwapTargetLeverageSnapshot {
        OkxSwapTargetLeverageSnapshot {
            account_id: "credential".to_owned(),
            product_id: "BTC-USDT-SWAP".to_owned(),
            target_leverage: decimal(target_leverage),
            target_qty: decimal(target_qty),
        }
    }

    #[test]
    fn matching_snapshot_avoids_passive_leverage_rebalancing() {
        let snapshot = snapshot(1, 10);
        assert_eq!(
            snapshot_decision(&config(1, Some(snapshot.clone())), decimal(10), false),
            SnapshotDecision::Use(snapshot)
        );
    }

    #[test]
    fn reversal_flattens_before_recalculating_leverage() {
        assert_eq!(
            snapshot_decision(&config(-1, Some(snapshot(1, 10))), decimal(10), false),
            SnapshotDecision::Flatten
        );
        assert_eq!(
            snapshot_decision(&config(-1, Some(snapshot(-1, -10))), Decimal::ZERO, false,),
            SnapshotDecision::Calculate
        );
    }

    #[test]
    fn changed_signal_recalculates_with_new_equity() {
        assert_eq!(
            snapshot_decision(&config(2, Some(snapshot(1, 10))), decimal(10), false),
            SnapshotDecision::Calculate
        );
    }

    #[test]
    fn pending_open_order_keeps_its_equity_snapshot_while_flat() {
        let snapshot = snapshot(1, 10);
        assert_eq!(
            snapshot_decision(&config(1, Some(snapshot.clone())), Decimal::ZERO, true),
            SnapshotDecision::Use(snapshot)
        );
    }

    #[test]
    fn target_quantity_uses_equity_contract_value_and_bbo() {
        let instrument = OkxInstrument {
            inst_id: "BTC-USDT-SWAP".to_owned(),
            lot_sz: "1".to_owned(),
            min_sz: "1".to_owned(),
            ct_val: "0.01".to_owned(),
        };
        assert_eq!(
            target_qty(decimal(2), decimal(1000), &instrument, decimal(100)).expect("quantity"),
            decimal(2000)
        );
        assert_eq!(
            target_qty(decimal(-1), decimal(1000), &instrument, decimal(100)).expect("quantity"),
            decimal(-1000)
        );
    }

    #[test]
    fn post_only_orders_use_bbo_and_reduce_only_when_closing() {
        let ticker = OkxTicker {
            inst_id: "BTC-USDT-SWAP".to_owned(),
            bid_px: "99".to_owned(),
            ask_px: "101".to_owned(),
        };
        let opening = plan_adjustment(&config(1, None), Decimal::ZERO, decimal(10)).expect("open");
        let opening_order = post_only_order(&opening, &ticker, None).expect("opening order");
        assert_eq!(opening_order.side, OkxOrderSide::Buy);
        assert_eq!(opening_order.ord_type, OkxOrderType::PostOnly);
        assert_eq!(opening_order.px.as_deref(), Some("99"));
        assert_eq!(opening_order.reduce_only, None);

        let closing = TargetPositionAdjustment {
            account_id: "credential".to_owned(),
            product_id: "BTC-USDT-SWAP".to_owned(),
            current_qty: decimal(10),
            target_qty: Decimal::ZERO,
            delta_qty: decimal(-10),
            order_qty: decimal(10),
            side: TargetPositionAdjustmentSide::Sell,
        };
        let closing_order = post_only_order(&closing, &ticker, None).expect("closing order");
        assert_eq!(closing_order.side, OkxOrderSide::Sell);
        assert_eq!(closing_order.px.as_deref(), Some("101"));
        assert_eq!(closing_order.reduce_only.as_deref(), Some("true"));
        assert_eq!(
            bbo_price(decimal(-1), &ticker).expect("short price"),
            decimal(101)
        );
    }
}
