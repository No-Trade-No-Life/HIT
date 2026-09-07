use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AccountCredential,
    exchanges::ctpd::{
        CtpdClient, CtpdCredential, CtpdInstrument, CtpdOffset, CtpdOrder, CtpdOrderDirection,
        CtpdPlaceOrderRequest, CtpdPosition, CtpdTick,
    },
    runtime::ResourceClaim,
};

use super::{Result, TraderRunError};

const CFFEX: &str = "CFFEX";

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CtpdCffexIndexFuturesHedgePriorityConfig {
    pub account_id: String,
    pub instrument_id: String,
    pub net_volume: i32,
}

impl CtpdCffexIndexFuturesHedgePriorityConfig {
    #[must_use]
    pub fn resource_claim(&self) -> ResourceClaim {
        ResourceClaim {
            key: format!("{}:{}", self.account_id, self.instrument_id),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CtpdPositionDirection {
    Long,
    Short,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CtpdCffexExecutionStage {
    CloseYesterday,
    OpenHedge,
    CloseToday,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CtpdCffexPlannedOrder {
    pub stage: CtpdCffexExecutionStage,
    pub direction: CtpdOrderDirection,
    pub offset: CtpdOffset,
    pub volume: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CtpdCffexIndexFuturesHedgePriorityRun {
    pub plan: Vec<CtpdCffexPlannedOrder>,
    pub orders: Vec<CtpdOrder>,
}

/// Validates the configuration and the dedicated CTPD credential shape.
///
/// # Errors
///
/// Returns an error when the model is not constrained to IF/IH/IC/IM or the credential does not
/// belong to CTPD.
pub fn validate_ctpd_cffex_index_futures_hedge_priority_config(
    config: &CtpdCffexIndexFuturesHedgePriorityConfig,
    credential: &AccountCredential,
) -> Result<()> {
    if !is_cffex_index_future(&config.instrument_id) {
        return Err(TraderRunError::CtpdCffexInvalidInstrument {
            instrument_id: config.instrument_id.clone(),
        });
    }
    target_volumes(config.net_volume)?;
    match credential {
        AccountCredential::CtpdHttpApiKeyV1 { base_url, api_key }
            if !base_url.is_empty() && !api_key.is_empty() =>
        {
            Ok(())
        }
        AccountCredential::CtpdHttpApiKeyV1 { .. } => {
            Err(TraderRunError::CtpdCffexInvalidCredential)
        }
        _ => Err(TraderRunError::CredentialMismatch),
    }
}

/// Builds the CFFEX-specific execution order: close yesterday, open the desired hedge, then
/// close today. A `close` request is CTPD's representation of the CTP `Close` offset used for
/// yesterday holdings; `close_today` is used only in the final stage.
///
/// # Errors
///
/// Returns an error if CTPD's position data is inconsistent or its per-order limit is invalid.
pub fn plan_ctpd_cffex_index_futures_hedge_priority_orders(
    config: &CtpdCffexIndexFuturesHedgePriorityConfig,
    positions: &[CtpdPosition],
    max_limit_order_volume: i32,
) -> Result<Vec<CtpdCffexPlannedOrder>> {
    if max_limit_order_volume <= 0 {
        return Err(TraderRunError::CtpdCffexInvalidOrderLimit {
            max_limit_order_volume,
        });
    }
    let (target_long_volume, target_short_volume) = target_volumes(config.net_volume)?;

    let long = position_summary(
        positions,
        &config.instrument_id,
        CtpdPositionDirection::Long,
    )?;
    let short = position_summary(
        positions,
        &config.instrument_id,
        CtpdPositionDirection::Short,
    )?;
    let long_excess = (long.volume - target_long_volume).max(0);
    let short_excess = (short.volume - target_short_volume).max(0);
    let long_close_yesterday = long.yesterday_volume.min(long_excess);
    let short_close_yesterday = short.yesterday_volume.min(short_excess);
    let long_after_close_yesterday = long.volume - long_close_yesterday;
    let short_after_close_yesterday = short.volume - short_close_yesterday;
    let long_open = (target_long_volume - long_after_close_yesterday).max(0);
    let short_open = (target_short_volume - short_after_close_yesterday).max(0);
    let long_close_today = (long_excess - long_close_yesterday).min(long.today_volume);
    let short_close_today = (short_excess - short_close_yesterday).min(short.today_volume);

    let mut plan = Vec::new();
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::CloseYesterday,
        CtpdOrderDirection::Sell,
        CtpdOffset::CloseYesterday,
        long_close_yesterday,
        max_limit_order_volume,
    );
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::CloseYesterday,
        CtpdOrderDirection::Buy,
        CtpdOffset::CloseYesterday,
        short_close_yesterday,
        max_limit_order_volume,
    );
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::OpenHedge,
        CtpdOrderDirection::Buy,
        CtpdOffset::Open,
        long_open,
        max_limit_order_volume,
    );
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::OpenHedge,
        CtpdOrderDirection::Sell,
        CtpdOffset::Open,
        short_open,
        max_limit_order_volume,
    );
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::CloseToday,
        CtpdOrderDirection::Sell,
        CtpdOffset::CloseToday,
        long_close_today,
        max_limit_order_volume,
    );
    append_orders(
        &mut plan,
        CtpdCffexExecutionStage::CloseToday,
        CtpdOrderDirection::Buy,
        CtpdOffset::CloseToday,
        short_close_today,
        max_limit_order_volume,
    );
    Ok(plan)
}

/// Executes one target-position cycle against CTPD.
///
/// # Errors
///
/// Returns an error if CTPD is not connected, logged in, and explicitly enabled for trading; if
/// the queried contract is not the configured CFFEX contract; or if a CTPD request is rejected.
pub async fn run_ctpd_cffex_index_futures_hedge_priority_once(
    config: &CtpdCffexIndexFuturesHedgePriorityConfig,
    credential: &AccountCredential,
) -> Result<CtpdCffexIndexFuturesHedgePriorityRun> {
    validate_ctpd_cffex_index_futures_hedge_priority_config(config, credential)?;
    let credential = ctpd_credential(credential)?;
    run_ctpd_cffex_index_futures_hedge_priority_once_with_client(
        config,
        &CtpdClient::new(credential)?,
    )
    .await
}

/// Executes one target-position cycle with an explicit CTPD client.
///
/// # Errors
///
/// Returns an error if CTPD is not ready, the contract cannot be verified, position data is
/// invalid, or an order cannot be accepted by CTPD. It waits for previously submitted orders to
/// become terminal before it submits the next execution stage.
pub async fn run_ctpd_cffex_index_futures_hedge_priority_once_with_client(
    config: &CtpdCffexIndexFuturesHedgePriorityConfig,
    client: &CtpdClient,
) -> Result<CtpdCffexIndexFuturesHedgePriorityRun> {
    let status = client.get_status().await?;
    if !status.connected || !status.logged_in || !status.trading_enabled {
        return Err(TraderRunError::CtpdCffexStatusNotReady {
            connected: status.connected,
            logged_in: status.logged_in,
            trading_enabled: status.trading_enabled,
        });
    }
    if client
        .get_orders()
        .await?
        .iter()
        .any(|order| order.instrument_id == config.instrument_id && !is_terminal_order(order))
    {
        return Ok(CtpdCffexIndexFuturesHedgePriorityRun {
            plan: Vec::new(),
            orders: Vec::new(),
        });
    }
    let instruments = client.get_instruments(&config.instrument_id).await?;
    let instrument = verify_instrument(config, &instruments)?;
    let plan = plan_ctpd_cffex_index_futures_hedge_priority_orders(
        config,
        &client.get_positions().await?,
        instrument.max_limit_order_volume,
    )?;
    let Some(stage) = plan.first().map(|item| item.stage) else {
        return Ok(CtpdCffexIndexFuturesHedgePriorityRun {
            plan,
            orders: Vec::new(),
        });
    };
    let plan = plan
        .into_iter()
        .take_while(|item| item.stage == stage)
        .collect::<Vec<_>>();
    let mut orders = Vec::with_capacity(plan.len());
    for item in &plan {
        let tick = client.next_tick(&config.instrument_id).await?;
        let request = CtpdPlaceOrderRequest {
            instrument_id: config.instrument_id.clone(),
            exchange_id: CFFEX.to_owned(),
            direction: item.direction,
            offset: item.offset,
            price: order_price(&tick, item.direction, instrument)?,
            volume: item.volume,
        };
        let idempotency_key = Uuid::new_v4().simple().to_string();
        orders.push(client.place_order(&idempotency_key, &request).await?);
    }
    Ok(CtpdCffexIndexFuturesHedgePriorityRun { plan, orders })
}

fn is_terminal_order(order: &CtpdOrder) -> bool {
    matches!(order.status.as_str(), "filled" | "cancelled")
}

#[derive(Debug, Clone, Copy, Default)]
struct PositionSummary {
    volume: i32,
    today_volume: i32,
    yesterday_volume: i32,
}

fn is_cffex_index_future(instrument_id: &str) -> bool {
    ["IF", "IH", "IC", "IM"].iter().any(|prefix| {
        instrument_id.strip_prefix(prefix).is_some_and(|suffix| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

fn ctpd_credential(credential: &AccountCredential) -> Result<CtpdCredential> {
    match credential {
        AccountCredential::CtpdHttpApiKeyV1 { base_url, api_key } => Ok(CtpdCredential {
            base_url: base_url.clone(),
            api_key: api_key.clone(),
        }),
        _ => Err(TraderRunError::CredentialMismatch),
    }
}

fn verify_instrument<'a>(
    config: &CtpdCffexIndexFuturesHedgePriorityConfig,
    instruments: &'a [CtpdInstrument],
) -> Result<&'a CtpdInstrument> {
    instruments
        .iter()
        .find(|instrument| {
            instrument.instrument_id == config.instrument_id && instrument.exchange_id == CFFEX
        })
        .ok_or_else(|| TraderRunError::CtpdCffexInstrumentNotFound {
            instrument_id: config.instrument_id.clone(),
        })
}

fn position_summary(
    positions: &[CtpdPosition],
    instrument_id: &str,
    direction: CtpdPositionDirection,
) -> Result<PositionSummary> {
    let expected_direction = match direction {
        CtpdPositionDirection::Long => "long",
        CtpdPositionDirection::Short => "short",
    };
    let mut summary = PositionSummary::default();
    for position in positions.iter().filter(|position| {
        position.instrument_id == instrument_id && position.direction == expected_direction
    }) {
        if position.volume < 0
            || position.today_volume < 0
            || position.today_volume > position.volume
        {
            return Err(TraderRunError::CtpdCffexInvalidPosition {
                instrument_id: instrument_id.to_owned(),
                direction: position.direction.clone(),
                volume: position.volume,
                today_volume: position.today_volume,
            });
        }
        summary.volume += position.volume;
        summary.today_volume += position.today_volume;
    }
    summary.yesterday_volume = summary.volume - summary.today_volume;
    Ok(summary)
}

fn append_orders(
    plan: &mut Vec<CtpdCffexPlannedOrder>,
    stage: CtpdCffexExecutionStage,
    direction: CtpdOrderDirection,
    offset: CtpdOffset,
    mut volume: i32,
    max_limit_order_volume: i32,
) {
    while volume > 0 {
        let order_volume = volume.min(max_limit_order_volume);
        plan.push(CtpdCffexPlannedOrder {
            stage,
            direction,
            offset,
            volume: order_volume,
        });
        volume -= order_volume;
    }
}

fn target_volumes(net_volume: i32) -> Result<(i32, i32)> {
    if net_volume >= 0 {
        return Ok((net_volume, 0));
    }
    let short_volume = net_volume
        .checked_abs()
        .ok_or(TraderRunError::CtpdCffexInvalidNetVolume { net_volume })?;
    Ok((0, short_volume))
}

fn order_price(
    tick: &CtpdTick,
    direction: CtpdOrderDirection,
    instrument: &CtpdInstrument,
) -> Result<f64> {
    let (side, price, volume) = match direction {
        CtpdOrderDirection::Buy => ("ask", tick.ask_price_1, tick.ask_volume_1),
        CtpdOrderDirection::Sell => ("bid", tick.bid_price_1, tick.bid_volume_1),
    };
    if price.is_finite() && price > 0.0 && price < f64::MAX && volume > 0 {
        return Ok(price);
    }
    Err(TraderRunError::CtpdCffexInvalidBbo {
        instrument_id: instrument.instrument_id.clone(),
        side,
        price,
        volume,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode, header},
        routing::get,
    };
    use serde_json::{Value, json};
    use tokio::{net::TcpListener, sync::Mutex};

    use super::*;

    fn config(net_volume: i32) -> CtpdCffexIndexFuturesHedgePriorityConfig {
        CtpdCffexIndexFuturesHedgePriorityConfig {
            account_id: "ctp-main".into(),
            instrument_id: "IF2609".into(),
            net_volume,
        }
    }

    fn position(direction: &str, volume: i32, today_volume: i32) -> CtpdPosition {
        CtpdPosition {
            instrument_id: "IF2609".into(),
            direction: direction.into(),
            volume,
            today_volume,
        }
    }

    #[test]
    fn reverses_long_position_in_cffex_fee_and_margin_order() -> Result<()> {
        let plan = plan_ctpd_cffex_index_futures_hedge_priority_orders(
            &config(-3),
            &[position("long", 5, 2)],
            100,
        )?;

        assert_eq!(
            plan,
            vec![
                CtpdCffexPlannedOrder {
                    stage: CtpdCffexExecutionStage::CloseYesterday,
                    direction: CtpdOrderDirection::Sell,
                    offset: CtpdOffset::CloseYesterday,
                    volume: 3,
                },
                CtpdCffexPlannedOrder {
                    stage: CtpdCffexExecutionStage::OpenHedge,
                    direction: CtpdOrderDirection::Sell,
                    offset: CtpdOffset::Open,
                    volume: 3,
                },
                CtpdCffexPlannedOrder {
                    stage: CtpdCffexExecutionStage::CloseToday,
                    direction: CtpdOrderDirection::Sell,
                    offset: CtpdOffset::CloseToday,
                    volume: 2,
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn retains_required_today_position_after_closing_yesterday_first() -> Result<()> {
        let plan = plan_ctpd_cffex_index_futures_hedge_priority_orders(
            &config(3),
            &[position("long", 9, 4)],
            100,
        )?;

        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].offset, CtpdOffset::CloseYesterday);
        assert_eq!(plan[0].volume, 5);
        assert_eq!(plan[1].offset, CtpdOffset::CloseToday);
        assert_eq!(plan[1].volume, 1);
        Ok(())
    }

    #[test]
    fn chunks_each_stage_without_reordering_stages() -> Result<()> {
        let plan = plan_ctpd_cffex_index_futures_hedge_priority_orders(
            &config(-3),
            &[position("long", 5, 2)],
            2,
        )?;

        assert_eq!(
            plan.iter().map(|item| item.stage).collect::<Vec<_>>(),
            vec![
                CtpdCffexExecutionStage::CloseYesterday,
                CtpdCffexExecutionStage::CloseYesterday,
                CtpdCffexExecutionStage::OpenHedge,
                CtpdCffexExecutionStage::OpenHedge,
                CtpdCffexExecutionStage::CloseToday,
            ]
        );
        assert!(plan.iter().all(|item| item.volume <= 2));
        Ok(())
    }

    #[test]
    fn bbo_taker_uses_best_opposing_price() -> Result<()> {
        let instrument = CtpdInstrument {
            instrument_id: "IF2609".into(),
            exchange_id: CFFEX.into(),
            price_tick: 0.2,
            max_limit_order_volume: 100,
        };
        let tick = CtpdTick {
            instrument_id: "IF2609".into(),
            bid_price_1: 3999.8,
            bid_volume_1: 10,
            ask_price_1: 4000.0,
            ask_volume_1: 12,
        };

        assert_eq!(
            order_price(&tick, CtpdOrderDirection::Buy, &instrument)?,
            4000.0
        );
        assert_eq!(
            order_price(&tick, CtpdOrderDirection::Sell, &instrument)?,
            3999.8
        );
        Ok(())
    }

    #[test]
    fn rejects_non_index_future_instrument() {
        let mut invalid = config(0);
        invalid.instrument_id = "rb2610".into();
        let credential = AccountCredential::CtpdHttpApiKeyV1 {
            base_url: "http://127.0.0.1:8080".into(),
            api_key: "key".into(),
        };
        assert!(matches!(
            validate_ctpd_cffex_index_futures_hedge_priority_config(&invalid, &credential),
            Err(TraderRunError::CtpdCffexInvalidInstrument { .. })
        ));
    }

    type MockOrderRequest = (String, String, String, f64);

    #[derive(Clone, Default)]
    struct MockCtpdState {
        requests: Arc<Mutex<Vec<MockOrderRequest>>>,
        outstanding: bool,
    }

    #[tokio::test]
    async fn runner_calls_ctpd_in_fixed_stage_order() -> Result<()> {
        let state = MockCtpdState::default();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/status", get(mock_status))
            .route("/v1/instruments", get(mock_instruments))
            .route("/v1/positions", get(mock_positions))
            .route("/v1/ticks", get(mock_tick))
            .route("/v1/orders", get(mock_orders).post(mock_order))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = CtpdClient::new(CtpdCredential {
            base_url: format!("http://{address}"),
            api_key: "ctpd-test-key".into(),
        })?;

        let run =
            run_ctpd_cffex_index_futures_hedge_priority_once_with_client(&config(-3), &client)
                .await?;
        server.abort();

        assert_eq!(run.orders.len(), 1);
        assert_eq!(
            *state.requests.lock().await,
            vec![(
                "Bearer ctpd-test-key".into(),
                "close".into(),
                "sell".into(),
                3999.8,
            )]
        );
        Ok(())
    }

    #[tokio::test]
    async fn runner_waits_for_existing_ctpd_order_before_the_next_stage() -> Result<()> {
        let state = MockCtpdState {
            requests: Arc::new(Mutex::new(Vec::new())),
            outstanding: true,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new()
            .route("/v1/status", get(mock_status))
            .route("/v1/orders", get(mock_orders))
            .with_state(state.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let client = CtpdClient::new(CtpdCredential {
            base_url: format!("http://{address}"),
            api_key: "ctpd-test-key".into(),
        })?;

        let run =
            run_ctpd_cffex_index_futures_hedge_priority_once_with_client(&config(-3), &client)
                .await?;
        server.abort();

        assert!(run.plan.is_empty());
        assert!(run.orders.is_empty());
        assert!(state.requests.lock().await.is_empty());
        Ok(())
    }

    async fn mock_status() -> Json<Value> {
        Json(json!({"connected": true, "logged_in": true, "trading_enabled": true}))
    }

    async fn mock_instruments() -> Json<Value> {
        Json(json!([{
            "instrument_id": "IF2609",
            "exchange_id": "CFFEX",
            "price_tick": 0.2,
            "max_limit_order_volume": 100
        }]))
    }

    async fn mock_positions() -> Json<Value> {
        Json(json!([{
            "instrument_id": "IF2609",
            "direction": "long",
            "volume": 5,
            "today_volume": 2
        }]))
    }

    async fn mock_tick() -> ([(header::HeaderName, &'static str); 1], String) {
        (
            [(header::CONTENT_TYPE, "text/event-stream")],
            "event: tick\ndata: {\"instrument_id\":\"IF2609\",\"bid_price_1\":3999.8,\"bid_volume_1\":10,\"ask_price_1\":4000.0,\"ask_volume_1\":12}\n\n".into(),
        )
    }

    async fn mock_orders(State(state): State<MockCtpdState>) -> Json<Value> {
        if state.outstanding {
            Json(json!([{
                "order_ref": "000000000001",
                "front_id": 1,
                "session_id": 1,
                "instrument_id": "IF2609",
                "exchange_id": "CFFEX",
                "direction": "sell",
                "offset": "close",
                "price": 4000.0,
                "volume": 3,
                "status": "accepted",
                "status_message": "accepted"
            }]))
        } else {
            Json(json!([]))
        }
    }

    async fn mock_order(
        State(state): State<MockCtpdState>,
        headers: HeaderMap,
        Json(request): Json<Value>,
    ) -> (StatusCode, Json<Value>) {
        let authorization = headers
            .get("authorization")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let idempotency_key = headers
            .get("idempotency-key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert_eq!(idempotency_key.len(), 32);
        state.requests.lock().await.push((
            authorization,
            request["offset"].as_str().unwrap().to_owned(),
            request["direction"].as_str().unwrap().to_owned(),
            request["price"].as_f64().unwrap(),
        ));
        (
            StatusCode::ACCEPTED,
            Json(json!({
                "order_ref": "000000000001",
                "front_id": 1,
                "session_id": 1,
                "instrument_id": "IF2609",
                "exchange_id": "CFFEX",
                "direction": request["direction"],
                "offset": request["offset"],
                "price": request["price"],
                "volume": request["volume"],
                "status": "accepted",
                "status_message": "accepted"
            })),
        )
    }
}
