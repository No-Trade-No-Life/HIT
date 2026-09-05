//! Fixed, exchange-specific execution templates imported from `traders` commit
//! `d368e39836a5779525b4844233728057904513ae`.

pub mod credential;
pub mod exchanges;
pub mod models;
pub mod runtime;

pub use credential::AccountCredential;
pub use models::{
    TraderModel, run_binance_um_futures_copy_target_position_bbo_maker_by_direction_once,
    run_binance_um_futures_copy_target_position_once,
    run_ctpd_cffex_index_futures_hedge_priority_once,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once,
    run_okx_swap_copy_target_position_bbo_maker_once,
    run_okx_swap_copy_target_position_multi_order_maker_by_direction_once,
    run_okx_swap_copy_target_position_multi_order_maker_once,
    run_okx_swap_copy_target_position_once,
};
