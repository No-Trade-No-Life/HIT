use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::exchanges::Result;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct BinanceUmFuturesCredential {
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Clone)]
pub struct BinanceUmFuturesClient {
    http: Client,
    credential: BinanceUmFuturesCredential,
    account_api: BinanceUmFuturesAccountApi,
    base_url: String,
    market_data_base_url: String,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinanceUmFuturesAccountApi {
    #[default]
    Fapi,
    PortfolioMargin,
}

impl BinanceUmFuturesAccountApi {
    #[must_use]
    pub const fn base_url(self) -> &'static str {
        match self {
            Self::Fapi => "https://fapi.binance.com",
            Self::PortfolioMargin => "https://papi.binance.com",
        }
    }

    const fn position_risk_path(self) -> &'static str {
        match self {
            Self::Fapi => "/fapi/v2/positionRisk",
            Self::PortfolioMargin => "/papi/v1/um/positionRisk",
        }
    }

    const fn order_path(self) -> &'static str {
        match self {
            Self::Fapi => "/fapi/v1/order",
            Self::PortfolioMargin => "/papi/v1/um/order",
        }
    }

    const fn position_side_dual_path(self) -> &'static str {
        match self {
            Self::Fapi => "/fapi/v1/positionSide/dual",
            Self::PortfolioMargin => "/papi/v1/um/positionSide/dual",
        }
    }

    const fn open_orders_path(self) -> &'static str {
        match self {
            Self::Fapi => "/fapi/v1/openOrders",
            Self::PortfolioMargin => "/papi/v1/um/openOrders",
        }
    }

    const fn market_data_base_url() -> &'static str {
        "https://fapi.binance.com"
    }

    const fn book_ticker_path() -> &'static str {
        "/fapi/v1/ticker/bookTicker"
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesPositionRisk {
    pub symbol: String,
    pub position_amt: String,
    pub entry_price: String,
    pub mark_price: String,
    pub un_realized_profit: String,
    pub position_side: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesMarketOrderRequest {
    pub symbol: String,
    pub side: BinanceUmFuturesOrderSide,
    #[serde(rename = "type")]
    pub order_type: BinanceUmFuturesOrderType,
    pub quantity: String,
    pub reduce_only: Option<bool>,
    pub position_side: Option<BinanceUmFuturesPositionSide>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BinanceUmFuturesPositionSide {
    Both,
    Long,
    Short,
}

impl BinanceUmFuturesPositionSide {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "BOTH",
            Self::Long => "LONG",
            Self::Short => "SHORT",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesLimitOrderRequest {
    pub symbol: String,
    pub side: BinanceUmFuturesOrderSide,
    #[serde(rename = "type")]
    pub order_type: BinanceUmFuturesOrderType,
    pub time_in_force: BinanceUmFuturesTimeInForce,
    pub quantity: String,
    pub price: String,
    pub reduce_only: Option<bool>,
    pub position_side: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesOpenOrder {
    pub symbol: String,
    pub order_id: u64,
    pub side: BinanceUmFuturesOrderSide,
    #[serde(rename = "type")]
    pub order_type: BinanceUmFuturesOrderType,
    pub time_in_force: String,
    pub price: String,
    pub orig_qty: String,
    pub position_side: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesBookTicker {
    pub symbol: String,
    pub bid_price: String,
    pub ask_price: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesPositionSideDual {
    pub dual_side_position: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub enum BinanceUmFuturesOrderSide {
    BUY,
    SELL,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub enum BinanceUmFuturesOrderType {
    MARKET,
    LIMIT,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum BinanceUmFuturesTimeInForce {
    GTX,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinanceUmFuturesOrderResponse {
    pub symbol: String,
    pub order_id: u64,
    pub client_order_id: String,
    pub status: String,
}

impl BinanceUmFuturesClient {
    #[must_use]
    pub fn new(credential: BinanceUmFuturesCredential) -> Self {
        Self::with_account_api(credential, BinanceUmFuturesAccountApi::default())
    }

    #[must_use]
    pub fn with_account_api(
        credential: BinanceUmFuturesCredential,
        account_api: BinanceUmFuturesAccountApi,
    ) -> Self {
        Self {
            http: Client::new(),
            credential,
            account_api,
            base_url: account_api.base_url().to_owned(),
            market_data_base_url: BinanceUmFuturesAccountApi::market_data_base_url().to_owned(),
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url.clone();
        self.market_data_base_url = base_url;
        self
    }

    /// Calls the configured Binance UM futures position risk endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn get_position_risk(
        &self,
        symbol: &str,
    ) -> Result<Vec<BinanceUmFuturesPositionRisk>> {
        self.signed_get(self.account_api.position_risk_path(), &[("symbol", symbol)])
            .await
    }

    /// Calls the configured Binance UM futures position mode endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn get_position_side_dual(&self) -> Result<BinanceUmFuturesPositionSideDual> {
        self.signed_get(self.account_api.position_side_dual_path(), &[])
            .await
    }

    /// Calls the configured Binance UM futures open-orders endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn get_open_orders(&self, symbol: &str) -> Result<Vec<BinanceUmFuturesOpenOrder>> {
        self.signed_get(self.account_api.open_orders_path(), &[("symbol", symbol)])
            .await
    }

    /// Calls the configured Binance UM futures book ticker endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if transport or response decoding fails.
    pub async fn get_book_ticker(&self, symbol: &str) -> Result<BinanceUmFuturesBookTicker> {
        let response = self
            .http
            .get(self.book_ticker_url())
            .query(&[("symbol", symbol)])
            .send()
            .await?;

        decode_response(response).await
    }

    /// Calls the configured Binance UM futures order endpoint for a market order.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn post_market_order(
        &self,
        order: &BinanceUmFuturesMarketOrderRequest,
    ) -> Result<BinanceUmFuturesOrderResponse> {
        let mut params = vec![
            ("symbol", order.symbol.as_str()),
            (
                "side",
                match order.side {
                    BinanceUmFuturesOrderSide::BUY => "BUY",
                    BinanceUmFuturesOrderSide::SELL => "SELL",
                },
            ),
            ("type", "MARKET"),
            ("quantity", order.quantity.as_str()),
        ];

        if let Some(position_side) = &order.position_side {
            params.push(("positionSide", position_side.as_str()));
        }

        if let Some(reduce_only) = order.reduce_only {
            params.push(("reduceOnly", if reduce_only { "true" } else { "false" }));
        }

        self.signed_post(self.account_api.order_path(), &params)
            .await
    }

    /// Calls the configured Binance UM futures order endpoint for a limit order.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn post_limit_order(
        &self,
        order: &BinanceUmFuturesLimitOrderRequest,
    ) -> Result<BinanceUmFuturesOrderResponse> {
        let mut params = vec![
            ("symbol", order.symbol.as_str()),
            (
                "side",
                match order.side {
                    BinanceUmFuturesOrderSide::BUY => "BUY",
                    BinanceUmFuturesOrderSide::SELL => "SELL",
                },
            ),
            ("type", "LIMIT"),
            ("timeInForce", "GTX"),
            ("quantity", order.quantity.as_str()),
            ("price", order.price.as_str()),
        ];

        if let Some(position_side) = &order.position_side {
            params.push(("positionSide", position_side.as_str()));
        }

        if let Some(reduce_only) = order.reduce_only {
            params.push(("reduceOnly", if reduce_only { "true" } else { "false" }));
        }

        self.signed_post(self.account_api.order_path(), &params)
            .await
    }

    /// Calls the configured Binance UM futures order deletion endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, encoding, transport, or response decoding fails.
    pub async fn delete_order(
        &self,
        symbol: &str,
        order_id: u64,
    ) -> Result<BinanceUmFuturesOrderResponse> {
        let order_id = order_id.to_string();
        self.signed_delete(
            self.account_api.order_path(),
            &[("symbol", symbol), ("orderId", order_id.as_str())],
        )
        .await
    }

    async fn signed_get<T>(&self, path: &str, params: &[(&str, &str)]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.signed_url(path, params)?;
        let response = self
            .http
            .get(url)
            .header("X-MBX-APIKEY", &self.credential.api_key)
            .send()
            .await?;

        decode_response(response).await
    }

    async fn signed_post<T>(&self, path: &str, params: &[(&str, &str)]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.signed_url(path, params)?;
        let response = self
            .http
            .post(url)
            .header("X-MBX-APIKEY", &self.credential.api_key)
            .send()
            .await?;

        decode_response(response).await
    }

    async fn signed_delete<T>(&self, path: &str, params: &[(&str, &str)]) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let url = self.signed_url(path, params)?;
        let response = self
            .http
            .delete(url)
            .header("X-MBX-APIKEY", &self.credential.api_key)
            .send()
            .await?;

        decode_response(response).await
    }

    fn signed_url(&self, path: &str, params: &[(&str, &str)]) -> Result<String> {
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let recv_window = "5000";
        let mut sign_params = params.to_vec();
        sign_params.push(("recvWindow", recv_window));
        sign_params.push(("timestamp", timestamp.as_str()));

        let query = serde_urlencoded::to_string(&sign_params)?;
        let signature = sign_hex(&self.credential.api_secret, &query)?;

        Ok(format!(
            "{}{}?{}&signature={}",
            self.base_url, path, query, signature
        ))
    }

    fn book_ticker_url(&self) -> String {
        format!(
            "{}{}",
            self.market_data_base_url,
            BinanceUmFuturesAccountApi::book_ticker_path()
        )
    }
}

async fn decode_response<T>(response: reqwest::Response) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(crate::exchanges::ExchangeApiError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }

    Ok(serde_json::from_str::<T>(&body)?)
}

fn sign_hex(secret: &str, payload: &str) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(payload.as_bytes());
    Ok(hex_lower(&mac.finalize().into_bytes()))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, routing::get};
    use serde_json::{Value, json};
    use tokio::net::TcpListener;

    use super::*;

    async fn mock_book_ticker() -> Json<Value> {
        Json(json!({
            "symbol": "ETHUSDC",
            "bidPrice": "1.00",
            "askPrice": "1.01"
        }))
    }

    #[tokio::test]
    async fn portfolio_margin_book_ticker_uses_fapi_market_data_endpoint() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route("/fapi/v1/ticker/bookTicker", get(mock_book_ticker));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = BinanceUmFuturesClient::with_account_api(
            BinanceUmFuturesCredential {
                api_key: "test-key".into(),
                api_secret: "test-secret".into(),
            },
            BinanceUmFuturesAccountApi::PortfolioMargin,
        );
        assert_eq!(
            client.book_ticker_url(),
            "https://fapi.binance.com/fapi/v1/ticker/bookTicker"
        );

        let ticker = client
            .with_base_url(format!("http://{address}"))
            .get_book_ticker("ETHUSDC")
            .await?;

        assert_eq!(ticker.symbol, "ETHUSDC");
        assert_eq!(ticker.bid_price, "1.00");
        assert_eq!(ticker.ask_price, "1.01");
        server.abort();
        Ok(())
    }
}
