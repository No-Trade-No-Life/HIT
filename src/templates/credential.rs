use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub account_id: String,

    #[serde(flatten)]
    pub credential: AccountCredential,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "credential_type")]
pub enum AccountCredential {
    #[serde(rename = "binance_um_futures.api_key_secret_v1")]
    BinanceUmFuturesApiKeySecretV1 { api_key: String, api_secret: String },

    #[serde(rename = "okx.api_key_secret_passphrase_v1")]
    OkxApiKeySecretPassphraseV1 {
        api_key: String,
        api_secret: String,
        passphrase: String,
    },

    #[serde(rename = "okx.demo.api_key_secret_passphrase_v1")]
    OkxDemoApiKeySecretPassphraseV1 {
        api_key: String,
        api_secret: String,
        passphrase: String,
    },

    #[serde(rename = "ctpd.http_api_key_v1")]
    CtpdHttpApiKeyV1 { base_url: String, api_key: String },
}

impl fmt::Debug for CredentialRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialRecord")
            .field("account_id", &self.account_id)
            .field("credential", &self.credential)
            .finish()
    }
}

impl fmt::Debug for AccountCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinanceUmFuturesApiKeySecretV1 { .. } => formatter
                .debug_struct("BinanceUmFuturesApiKeySecretV1")
                .field("api_key", &"<redacted>")
                .field("api_secret", &"<redacted>")
                .finish(),
            Self::OkxApiKeySecretPassphraseV1 { .. } => formatter
                .debug_struct("OkxApiKeySecretPassphraseV1")
                .field("api_key", &"<redacted>")
                .field("api_secret", &"<redacted>")
                .field("passphrase", &"<redacted>")
                .finish(),
            Self::OkxDemoApiKeySecretPassphraseV1 { .. } => formatter
                .debug_struct("OkxDemoApiKeySecretPassphraseV1")
                .field("api_key", &"<redacted>")
                .field("api_secret", &"<redacted>")
                .field("passphrase", &"<redacted>")
                .finish(),
            Self::CtpdHttpApiKeyV1 { base_url, .. } => formatter
                .debug_struct("CtpdHttpApiKeyV1")
                .field("base_url", base_url)
                .field("api_key", &"<redacted>")
                .finish(),
        }
    }
}
