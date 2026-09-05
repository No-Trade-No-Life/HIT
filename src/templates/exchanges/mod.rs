pub mod binance_um_futures;
pub mod ctpd;
pub mod okx;

#[derive(Debug, thiserror::Error)]
pub enum ExchangeApiError {
    #[error("failed to build request signature")]
    Signature(#[from] hmac::digest::InvalidLength),

    #[error("failed to encode request query")]
    Query(#[from] serde_urlencoded::ser::Error),

    #[error("failed to encode request JSON")]
    Json(#[from] serde_json::Error),

    #[error("failed to build request header")]
    Header(#[from] reqwest::header::InvalidHeaderValue),

    #[error("exchange HTTP request failed")]
    Http(#[from] reqwest::Error),

    #[error("exchange HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },
}

pub type Result<T> = std::result::Result<T, ExchangeApiError>;
