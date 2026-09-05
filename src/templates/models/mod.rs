mod binance_um_futures_copy_target_position_bbo_maker_by_direction_20260605;
mod binance_um_futures_copy_target_position_v1;
mod ctpd_cffex_index_futures_hedge_priority_20260719;
mod okx_swap_copy_target_position_bbo_maker_20260605;
mod okx_swap_copy_target_position_bbo_maker_by_direction_20260605;
mod okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_20260609;
mod okx_swap_copy_target_position_multi_order_maker_20260605;
mod okx_swap_copy_target_position_multi_order_maker_by_direction_20260605;
mod okx_swap_copy_target_position_v1;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub use crate::exchanges::binance_um_futures::{
    BinanceUmFuturesAccountApi, BinanceUmFuturesPositionSide,
};
pub use binance_um_futures_copy_target_position_bbo_maker_by_direction_20260605::{
    BinanceUmFuturesBboMakerDirectionRun,
    BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    BinanceUmFuturesCopyTargetPositionBboMakerByDirectionRun, BinanceUmFuturesPositionDirection,
    run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once,
    run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once_with_client,
    validate_binance_um_futures_copy_target_position_bbo_maker_by_direction_config,
    validate_binance_um_futures_copy_target_position_bbo_maker_by_direction_config_with_client,
};
pub use binance_um_futures_copy_target_position_v1::{
    BinanceUmFuturesCopyTargetPositionConfig, BinanceUmFuturesCopyTargetPositionRun,
    plan_binance_um_futures_copy_target_position_adjustment,
    run_binance_um_futures_copy_target_position_once,
    run_binance_um_futures_copy_target_position_once_with_client,
};
pub use ctpd_cffex_index_futures_hedge_priority_20260719::{
    CtpdCffexExecutionStage, CtpdCffexIndexFuturesHedgePriorityConfig,
    CtpdCffexIndexFuturesHedgePriorityRun, CtpdCffexPlannedOrder, CtpdPositionDirection,
    plan_ctpd_cffex_index_futures_hedge_priority_orders,
    run_ctpd_cffex_index_futures_hedge_priority_once,
    run_ctpd_cffex_index_futures_hedge_priority_once_with_client,
    validate_ctpd_cffex_index_futures_hedge_priority_config,
};
pub use okx_swap_copy_target_position_bbo_maker_20260605::{
    OkxSwapCopyTargetPositionBboMakerConfig, OkxSwapCopyTargetPositionBboMakerRun,
    run_okx_swap_copy_target_position_bbo_maker_once,
    run_okx_swap_copy_target_position_bbo_maker_once_with_client,
};
pub use okx_swap_copy_target_position_bbo_maker_by_direction_20260605::{
    OkxSwapBboMakerDirectionRun, OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    OkxSwapCopyTargetPositionBboMakerByDirectionRun, OkxSwapPositionDirection,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client_and_positions,
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_config,
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_config_with_client,
};
pub use okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_20260609::{
    OkxPositionsSingleflightFetcher,
    OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig,
    OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightRun,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client_and_fetcher,
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_config,
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_config_with_client,
};
pub use okx_swap_copy_target_position_multi_order_maker_20260605::{
    NetMultiOrderPlan, OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    OkxSwapCopyTargetPositionMultiOrderMakerRun,
    run_okx_swap_copy_target_position_multi_order_maker_once,
    run_okx_swap_copy_target_position_multi_order_maker_once_with_client,
    validate_okx_swap_copy_target_position_multi_order_maker_config,
    validate_okx_swap_copy_target_position_multi_order_maker_config_with_client,
};
pub use okx_swap_copy_target_position_multi_order_maker_by_direction_20260605::{
    OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    OkxSwapCopyTargetPositionMultiOrderMakerByDirectionRun, OkxSwapMultiOrderMakerDirectionRun,
    OkxSwapMultiOrderPositionDirection,
    run_okx_swap_copy_target_position_multi_order_maker_by_direction_once,
    run_okx_swap_copy_target_position_multi_order_maker_by_direction_once_with_client,
};
pub use okx_swap_copy_target_position_v1::{
    OkxSwapCopyTargetPositionConfig, OkxSwapCopyTargetPositionRun,
    plan_okx_swap_copy_target_position_adjustment, run_okx_swap_copy_target_position_once,
    run_okx_swap_copy_target_position_once_with_client,
};

#[derive(Debug, thiserror::Error)]
pub enum TraderRunError {
    #[error("credential type does not match trader")]
    CredentialMismatch,

    #[error("failed to parse decimal from exchange response")]
    Decimal(#[from] rust_decimal::Error),

    #[error("exchange API call failed: {0}")]
    ExchangeApi(#[from] crate::exchanges::ExchangeApiError),

    #[error("exchange API call failed: {message}")]
    ExchangeApiMessage { message: String },

    #[error("OKX API returned code {code}: {message}")]
    OkxApi { code: String, message: String },

    #[error("OKX order rejected with code {code}: {message}")]
    OkxOrderRejected { code: String, message: String },

    #[error("OKX order response did not include an order result")]
    OkxOrderMissing,

    #[error("OKX response did not include {data}")]
    OkxDataMissing { data: &'static str },

    #[error("OKX account position mode must be {expected}, got {actual}")]
    OkxAccountModeMismatch {
        expected: &'static str,
        actual: String,
    },

    #[error("Binance UM Futures account must be in Hedge Mode")]
    BinanceUmFuturesAccountModeMismatch,

    #[error("CTPD CFFEX trader only accepts IF, IH, IC, or IM contracts, got {instrument_id}")]
    CtpdCffexInvalidInstrument { instrument_id: String },

    #[error("CTPD CFFEX trader target volumes must be non-negative")]
    CtpdCffexInvalidTarget,

    #[error("CTPD CFFEX trader buy_price and sell_price must be positive")]
    CtpdCffexInvalidPrice,

    #[error("CTPD CFFEX trader credential must contain a base URL and API key")]
    CtpdCffexInvalidCredential,

    #[error(
        "CTPD is not ready: connected={connected}, logged_in={logged_in}, trading_enabled={trading_enabled}"
    )]
    CtpdCffexStatusNotReady {
        connected: bool,
        logged_in: bool,
        trading_enabled: bool,
    },

    #[error("CTPD did not return CFFEX contract {instrument_id}")]
    CtpdCffexInstrumentNotFound { instrument_id: String },

    #[error(
        "CTPD returned invalid {direction} position for {instrument_id}: volume={volume}, today_volume={today_volume}"
    )]
    CtpdCffexInvalidPosition {
        instrument_id: String,
        direction: String,
        volume: i32,
        today_volume: i32,
    },

    #[error("CTPD returned invalid max_limit_order_volume={max_limit_order_volume}")]
    CtpdCffexInvalidOrderLimit { max_limit_order_volume: i32 },

    #[error(
        "CTPD contract {instrument_id} does not accept configured {side} price {price} at tick {price_tick}"
    )]
    CtpdCffexInvalidPriceTick {
        instrument_id: String,
        side: &'static str,
        price: Decimal,
        price_tick: f64,
    },
}

pub type Result<T> = std::result::Result<T, TraderRunError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "model", content = "config")]
pub enum TraderModel {
    #[serde(rename = "copy_target_position.binance.um_futures.20260605")]
    BinanceUmFuturesCopyTargetPositionV1(BinanceUmFuturesCopyTargetPositionConfig),

    #[serde(rename = "copy_target_position_bbo_maker_by_direction.binance.um_futures.20260605")]
    BinanceUmFuturesCopyTargetPositionBboMakerByDirection20260605(
        BinanceUmFuturesCopyTargetPositionBboMakerByDirectionConfig,
    ),

    #[serde(rename = "copy_target_position.ctpd.cffex_index_futures.20260719")]
    CtpdCffexIndexFuturesHedgePriority20260719(CtpdCffexIndexFuturesHedgePriorityConfig),

    #[serde(rename = "copy_target_position.okx.swap.20260605")]
    OkxSwapCopyTargetPositionV1(OkxSwapCopyTargetPositionConfig),

    #[serde(rename = "copy_target_position_bbo_maker.okx.swap.20260605")]
    OkxSwapCopyTargetPositionBboMaker20260605(OkxSwapCopyTargetPositionBboMakerConfig),

    #[serde(rename = "copy_target_position_bbo_maker_by_direction.okx.swap.20260605")]
    OkxSwapCopyTargetPositionBboMakerByDirection20260605(
        OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    ),

    #[serde(rename = "copy_target_position_bbo_maker_by_direction_singleflight.okx.swap.20260609")]
    OkxSwapCopyTargetPositionBboMakerByDirectionSingleflight20260609(
        OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig,
    ),

    #[serde(rename = "copy_target_position_multi_order_maker.okx.swap.20260605")]
    OkxSwapCopyTargetPositionMultiOrderMaker20260605(
        OkxSwapCopyTargetPositionMultiOrderMakerConfig,
    ),

    #[serde(rename = "copy_target_position_multi_order_maker_by_direction.okx.swap.20260605")]
    OkxSwapCopyTargetPositionMultiOrderMakerByDirection20260605(
        OkxSwapCopyTargetPositionMultiOrderMakerByDirectionConfig,
    ),
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TargetPositionAdjustment {
    pub account_id: String,
    pub product_id: String,
    pub current_qty: Decimal,
    pub target_qty: Decimal,
    pub delta_qty: Decimal,
    pub order_qty: Decimal,
    pub side: TargetPositionAdjustmentSide,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum TargetPositionAdjustmentSide {
    Buy,
    Sell,
}

fn plan_target_position_adjustment(
    account_id: &str,
    product_id: &str,
    target_qty: Decimal,
    tolerance_qty: Decimal,
    max_order_qty: Option<Decimal>,
    current_qty: Decimal,
) -> Option<TargetPositionAdjustment> {
    let delta_qty = target_qty - current_qty;
    let abs_delta_qty = delta_qty.abs();

    if abs_delta_qty <= tolerance_qty {
        return None;
    }

    let order_qty = match max_order_qty {
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
        account_id: account_id.to_owned(),
        product_id: product_id.to_owned(),
        current_qty,
        target_qty,
        delta_qty,
        order_qty,
        side,
    })
}
