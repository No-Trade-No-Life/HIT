use reqwest::{
    Client, Response,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize};

use crate::exchanges::{ExchangeApiError, Result};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CtpdCredential {
    pub base_url: String,
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct CtpdClient {
    http: Client,
    credential: CtpdCredential,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct CtpdStatus {
    pub connected: bool,
    pub logged_in: bool,
    pub trading_enabled: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct CtpdPosition {
    pub instrument_id: String,
    pub direction: String,
    pub volume: i32,
    pub today_volume: i32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CtpdInstrument {
    pub instrument_id: String,
    pub exchange_id: String,
    pub price_tick: f64,
    pub max_limit_order_volume: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CtpdPlaceOrderRequest {
    pub instrument_id: String,
    pub exchange_id: String,
    pub direction: CtpdOrderDirection,
    pub offset: CtpdOffset,
    pub price: f64,
    pub volume: i32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CtpdOrderDirection {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
pub enum CtpdOffset {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "close")]
    CloseYesterday,
    #[serde(rename = "close_today")]
    CloseToday,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CtpdOrder {
    pub order_ref: String,
    pub front_id: i32,
    pub session_id: i32,
    pub instrument_id: String,
    pub exchange_id: String,
    pub direction: String,
    pub offset: String,
    pub price: f64,
    pub volume: i32,
    pub status: String,
    pub status_message: String,
}

impl CtpdClient {
    /// Creates a direct HTTP client for one CTPD service.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(credential: CtpdCredential) -> Result<Self> {
        Ok(Self {
            // CTPD is normally a loopback service next to the trading program. Routing its
            // authenticated control plane through a process-wide HTTP proxy is unsafe.
            http: Client::builder().no_proxy().build()?,
            credential,
        })
    }

    /// Calls CTPD `GET /v1/status`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or CTPD returns an error response.
    pub async fn get_status(&self) -> Result<CtpdStatus> {
        decode_response(
            self.request(reqwest::Method::GET, "/v1/status")?
                .send()
                .await?,
        )
        .await
    }

    /// Calls CTPD `GET /v1/positions`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or CTPD returns an error response.
    pub async fn get_positions(&self) -> Result<Vec<CtpdPosition>> {
        decode_response(
            self.request(reqwest::Method::GET, "/v1/positions")?
                .send()
                .await?,
        )
        .await
    }

    /// Calls CTPD `GET /v1/instruments?query=...`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or CTPD returns an error response.
    pub async fn get_instruments(&self, query: &str) -> Result<Vec<CtpdInstrument>> {
        decode_response(
            self.request(reqwest::Method::GET, "/v1/instruments")?
                .query(&[("query", query)])
                .send()
                .await?,
        )
        .await
    }

    /// Calls CTPD `GET /v1/orders`.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or CTPD returns an error response.
    pub async fn get_orders(&self) -> Result<Vec<CtpdOrder>> {
        decode_response(
            self.request(reqwest::Method::GET, "/v1/orders")?
                .send()
                .await?,
        )
        .await
    }

    /// Calls CTPD `POST /v1/orders` with a caller-owned idempotency key.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP request fails or CTPD returns an error response.
    pub async fn place_order(
        &self,
        idempotency_key: &str,
        order: &CtpdPlaceOrderRequest,
    ) -> Result<CtpdOrder> {
        decode_response(
            self.request(reqwest::Method::POST, "/v1/orders")?
                .header("Idempotency-Key", idempotency_key)
                .json(order)
                .send()
                .await?,
        )
        .await
    }

    fn request(&self, method: reqwest::Method, path: &str) -> Result<reqwest::RequestBuilder> {
        Ok(self
            .http
            .request(
                method,
                format!("{}{}", self.credential.base_url.trim_end_matches('/'), path),
            )
            .headers(self.auth_headers()?))
    }

    fn auth_headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str(&format!("Bearer {}", self.credential.api_key))?;
        headers.insert(AUTHORIZATION, value);
        Ok(headers)
    }
}

async fn decode_response<T>(response: Response) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(ExchangeApiError::HttpStatus {
            status: status.as_u16(),
            body,
        });
    }
    Ok(serde_json::from_str(&body)?)
}
