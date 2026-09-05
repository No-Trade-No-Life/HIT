use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::SecondsFormat;
use hmac::{Hmac, Mac};
use reqwest::{
    Client,
    header::{HeaderValue, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::exchanges::Result;

const OKX_USER_AGENT: &str = concat!("traders/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct OkxCredential {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
}

#[derive(Debug, Clone)]
pub struct OkxClient {
    http: Client,
    credential: OkxCredential,
    base_url: String,
    simulated: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct OkxResponse<T> {
    pub code: String,
    pub msg: String,
    pub data: T,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxAccountConfig {
    pub pos_mode: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxPosition {
    pub inst_id: String,
    pub inst_type: String,
    pub pos: String,
    pub pos_side: String,
    pub avg_px: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxPlaceOrderRequest {
    pub inst_id: String,
    pub td_mode: String,
    pub side: OkxOrderSide,
    pub ord_type: OkxOrderType,
    pub sz: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub px: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos_side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduce_only: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
}

impl OkxPlaceOrderRequest {
    #[must_use]
    pub fn cross_post_only_limit(
        inst_id: String,
        side: OkxOrderSide,
        sz: String,
        px: String,
        pos_side: Option<String>,
        tag: Option<String>,
    ) -> Self {
        Self {
            inst_id,
            td_mode: "cross".to_owned(),
            side,
            ord_type: OkxOrderType::PostOnly,
            sz,
            px: Some(px),
            pos_side,
            reduce_only: None,
            tag,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OkxOrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub enum OkxOrderType {
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "post_only")]
    PostOnly,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxPlaceOrderResult {
    pub ord_id: String,
    pub cl_ord_id: String,
    pub s_code: String,
    pub s_msg: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxPendingOrder {
    pub inst_id: String,
    pub ord_id: String,
    pub side: OkxOrderSide,
    pub ord_type: OkxOrderType,
    pub px: String,
    pub sz: String,
    pub pos_side: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxCancelOrderRequest {
    pub inst_id: String,
    pub ord_id: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxCancelOrderResult {
    pub ord_id: String,
    pub cl_ord_id: String,
    pub s_code: String,
    pub s_msg: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxTicker {
    pub inst_id: String,
    pub bid_px: String,
    pub ask_px: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OkxInstrument {
    pub inst_id: String,
    pub lot_sz: String,
    pub min_sz: String,
}

impl OkxClient {
    #[must_use]
    pub fn new(credential: OkxCredential) -> Self {
        Self {
            http: Client::new(),
            credential,
            base_url: "https://www.okx.com".to_owned(),
            simulated: false,
        }
    }

    #[must_use]
    pub fn new_simulated(credential: OkxCredential) -> Self {
        Self {
            http: Client::new(),
            credential,
            base_url: "https://www.okx.com".to_owned(),
            simulated: true,
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    /// Calls `GET /api/v5/account/config`.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, transport, or response decoding fails.
    pub async fn get_account_config(&self) -> Result<OkxResponse<Vec<OkxAccountConfig>>> {
        self.signed_get("/api/v5/account/config").await
    }

    /// Calls `GET /api/v5/account/positions`.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, transport, or response decoding fails.
    pub async fn get_positions(
        &self,
        inst_type: &str,
        inst_id: &str,
    ) -> Result<OkxResponse<Vec<OkxPosition>>> {
        let query = format!("instType={inst_type}&instId={inst_id}");
        self.signed_get(&format!("/api/v5/account/positions?{query}"))
            .await
    }

    /// Calls `POST /api/v5/trade/order`.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, serialization, transport, or response decoding fails.
    pub async fn post_trade_order(
        &self,
        order: &OkxPlaceOrderRequest,
    ) -> Result<OkxResponse<Vec<OkxPlaceOrderResult>>> {
        self.signed_post("/api/v5/trade/order", order).await
    }

    /// Calls `GET /api/v5/trade/orders-pending`.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, transport, or response decoding fails.
    pub async fn get_orders_pending(
        &self,
        inst_type: &str,
        inst_id: &str,
    ) -> Result<OkxResponse<Vec<OkxPendingOrder>>> {
        let query = format!("instType={inst_type}&instId={inst_id}");
        self.signed_get(&format!("/api/v5/trade/orders-pending?{query}"))
            .await
    }

    /// Calls `POST /api/v5/trade/cancel-order`.
    ///
    /// # Errors
    ///
    /// Returns an error if signing, serialization, transport, or response decoding fails.
    pub async fn post_cancel_order(
        &self,
        order: &OkxCancelOrderRequest,
    ) -> Result<OkxResponse<Vec<OkxCancelOrderResult>>> {
        self.signed_post("/api/v5/trade/cancel-order", order).await
    }

    /// Calls `GET /api/v5/market/ticker`.
    ///
    /// # Errors
    ///
    /// Returns an error if transport or response decoding fails.
    pub async fn get_ticker(&self, inst_id: &str) -> Result<OkxResponse<Vec<OkxTicker>>> {
        self.public_get(&format!("/api/v5/market/ticker?instId={inst_id}"))
            .await
    }

    /// Calls `GET /api/v5/public/instruments`.
    ///
    /// # Errors
    ///
    /// Returns an error if transport or response decoding fails.
    pub async fn get_instruments(
        &self,
        inst_type: &str,
        inst_id: &str,
    ) -> Result<OkxResponse<Vec<OkxInstrument>>> {
        let query = format!("instType={inst_type}&instId={inst_id}");
        self.public_get(&format!("/api/v5/public/instruments?{query}"))
            .await
    }

    async fn public_get<T>(&self, path_with_query: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path_with_query))
            .header(USER_AGENT, OKX_USER_AGENT)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json::<T>().await?)
    }

    async fn signed_get<T>(&self, path_with_query: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        let timestamp = okx_timestamp();
        let signature = sign_base64(
            &self.credential.api_secret,
            &format!("{timestamp}GET{path_with_query}"),
        )?;
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path_with_query))
            .headers(self.auth_headers(&timestamp, &signature)?)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json::<T>().await?)
    }

    async fn signed_post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
        B: Serialize,
    {
        let body_json = serde_json::to_string(body)?;
        let timestamp = okx_timestamp();
        let signature = sign_base64(
            &self.credential.api_secret,
            &format!("{timestamp}POST{path}{body_json}"),
        )?;
        let response = self
            .http
            .post(format!("{}{}", self.base_url, path))
            .headers(self.auth_headers(&timestamp, &signature)?)
            .body(body_json)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json::<T>().await?)
    }

    fn auth_headers(&self, timestamp: &str, signature: &str) -> Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert(USER_AGENT, HeaderValue::from_static(OKX_USER_AGENT));
        headers.insert(
            "OK-ACCESS-KEY",
            HeaderValue::from_str(&self.credential.api_key)?,
        );
        headers.insert("OK-ACCESS-SIGN", HeaderValue::from_str(signature)?);
        headers.insert("OK-ACCESS-TIMESTAMP", HeaderValue::from_str(timestamp)?);
        headers.insert(
            "OK-ACCESS-PASSPHRASE",
            HeaderValue::from_str(&self.credential.passphrase)?,
        );
        if self.simulated {
            headers.insert("x-simulated-trading", HeaderValue::from_static("1"));
        }
        Ok(headers)
    }
}

fn okx_timestamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn sign_base64(secret: &str, payload: &str) -> Result<String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    mac.update(payload.as_bytes());
    Ok(STANDARD.encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use reqwest::header::USER_AGENT;

    use super::{OKX_USER_AGENT, OkxClient, OkxCredential};

    fn credential() -> OkxCredential {
        OkxCredential {
            api_key: "key".to_owned(),
            api_secret: "secret".to_owned(),
            passphrase: "passphrase".to_owned(),
        }
    }

    #[test]
    fn live_client_omits_simulated_trading_header() {
        let headers = OkxClient::new(credential())
            .auth_headers("timestamp", "signature")
            .expect("headers");
        assert!(!headers.contains_key("x-simulated-trading"));
        assert_eq!(headers[USER_AGENT], OKX_USER_AGENT);
    }

    #[test]
    fn simulated_client_sets_simulated_trading_header() {
        let headers = OkxClient::new_simulated(credential())
            .auth_headers("timestamp", "signature")
            .expect("headers");
        assert_eq!(headers["x-simulated-trading"], "1");
    }
}
