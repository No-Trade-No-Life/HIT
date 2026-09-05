use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex, Notify};

use crate::AccountCredential;
use crate::exchanges::okx::{OkxClient, OkxCredential, OkxPosition};

use super::{
    OkxSwapCopyTargetPositionBboMakerByDirectionConfig,
    OkxSwapCopyTargetPositionBboMakerByDirectionRun, Result, TraderRunError,
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client_and_positions,
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_config_with_client,
};

pub type OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig =
    OkxSwapCopyTargetPositionBboMakerByDirectionConfig;
pub type OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightRun =
    OkxSwapCopyTargetPositionBboMakerByDirectionRun;

type SharedPositionsResult = std::result::Result<Arc<Vec<OkxPosition>>, Arc<String>>;

#[derive(Debug, Clone, Default)]
pub struct OkxPositionsSingleflightFetcher {
    state: Arc<Mutex<OkxPositionsSingleflightState>>,
}

#[derive(Debug, Default)]
struct OkxPositionsSingleflightState {
    requests: HashMap<String, Arc<OkxPositionsSingleflightRequest>>,
    next_generation: u64,
}

#[derive(Debug)]
struct OkxPositionsSingleflightRequest {
    generation: u64,
    result: Mutex<Option<SharedPositionsResult>>,
    notify: Notify,
}

enum OkxPositionsSingleflightRole {
    Owner(Arc<OkxPositionsSingleflightRequest>),
    Waiter(Arc<OkxPositionsSingleflightRequest>),
}

impl OkxPositionsSingleflightFetcher {
    /// Returns fresh OKX SWAP positions while reusing only an in-flight request for the same account.
    ///
    /// # Errors
    ///
    /// Returns an error if the OKX API call fails or OKX returns a non-success code.
    pub async fn get_swap_positions(
        &self,
        account_id: &str,
        client: &OkxClient,
    ) -> Result<Arc<Vec<OkxPosition>>> {
        let key = format!("{account_id}:SWAP");
        let role = self.claim_request(&key).await;
        match role {
            OkxPositionsSingleflightRole::Owner(request) => {
                eprintln!(
                    "okx_positions_singleflight owner get_positions key={} generation={}",
                    key, request.generation
                );
                let result = fetch_swap_positions(client).await;
                *request.result.lock().await = Some(result.clone());
                self.finish_request(&key, request.generation).await;
                request.notify.notify_waiters();
                eprintln!(
                    "okx_positions_singleflight complete key={} generation={}",
                    key, request.generation
                );
                shared_positions_result(result)
            }
            OkxPositionsSingleflightRole::Waiter(request) => {
                eprintln!(
                    "okx_positions_singleflight reuse_inflight key={} generation={}",
                    key, request.generation
                );
                let result = wait_for_positions_result(&request).await;
                shared_positions_result(result)
            }
        }
    }

    async fn claim_request(&self, key: &str) -> OkxPositionsSingleflightRole {
        let mut state = self.state.lock().await;
        if let Some(request) = state.requests.get(key) {
            return OkxPositionsSingleflightRole::Waiter(Arc::clone(request));
        }

        let request = Arc::new(OkxPositionsSingleflightRequest {
            generation: state.next_generation,
            result: Mutex::new(None),
            notify: Notify::new(),
        });
        state.next_generation += 1;
        state.requests.insert(key.to_owned(), Arc::clone(&request));
        OkxPositionsSingleflightRole::Owner(request)
    }

    async fn finish_request(&self, key: &str, generation: u64) {
        let mut state = self.state.lock().await;
        if state
            .requests
            .get(key)
            .is_some_and(|request| request.generation == generation)
        {
            state.requests.remove(key);
        }
    }
}

async fn wait_for_positions_result(
    request: &OkxPositionsSingleflightRequest,
) -> SharedPositionsResult {
    loop {
        let notified = request.notify.notified();
        if let Some(result) = request.result.lock().await.clone() {
            return result;
        }
        notified.await;
    }
}

/// Checks account-level requirements for this model before a config is stored.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, OKX cannot return account config, or the
/// account is not in long/short position mode.
pub async fn validate_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_config(
    credential: &AccountCredential,
) -> Result<()> {
    let client = okx_client_from_credential(credential)?;
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_config_with_client(
        &client,
    )
    .await
}

/// Checks account-level requirements with an explicit OKX client.
///
/// # Errors
///
/// Returns an error if OKX cannot return account config, or the account is not in long/short
/// position mode.
pub async fn validate_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_config_with_client(
    client: &OkxClient,
) -> Result<()> {
    validate_okx_swap_copy_target_position_bbo_maker_by_direction_config_with_client(client).await
}

/// Runs one OKX swap BBO-maker-by-direction singleflight target-position copy cycle.
///
/// # Errors
///
/// Returns an error if the credential type is wrong, the OKX response cannot be parsed, the OKX API
/// returns an error code, or an order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig,
    credential: &AccountCredential,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightRun> {
    let client = okx_client_from_credential(credential)?;
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client(
        config, &client,
    )
    .await
}

/// Runs one OKX swap BBO-maker-by-direction singleflight cycle with an explicit API client.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig,
    client: &OkxClient,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightRun> {
    let fetcher = global_positions_singleflight_fetcher();
    run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client_and_fetcher(
        config,
        client,
        fetcher,
    )
    .await
}

/// Runs one OKX swap BBO-maker-by-direction singleflight cycle with an explicit fetcher.
///
/// # Errors
///
/// Returns an error if the OKX response cannot be parsed, the OKX API returns an error code, or an
/// order/cancel request is rejected.
pub async fn run_okx_swap_copy_target_position_bbo_maker_by_direction_singleflight_once_with_client_and_fetcher(
    config: &OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightConfig,
    client: &OkxClient,
    fetcher: &OkxPositionsSingleflightFetcher,
) -> Result<OkxSwapCopyTargetPositionBboMakerByDirectionSingleflightRun> {
    let positions = fetcher
        .get_swap_positions(&config.account_id, client)
        .await?;
    run_okx_swap_copy_target_position_bbo_maker_by_direction_once_with_client_and_positions(
        config,
        client,
        positions.as_ref().clone(),
    )
    .await
}

fn global_positions_singleflight_fetcher() -> &'static OkxPositionsSingleflightFetcher {
    static FETCHER: OnceLock<OkxPositionsSingleflightFetcher> = OnceLock::new();
    FETCHER.get_or_init(OkxPositionsSingleflightFetcher::default)
}

async fn fetch_swap_positions(client: &OkxClient) -> SharedPositionsResult {
    let response = client
        .get_positions("SWAP", "")
        .await
        .map_err(|error| Arc::new(error.to_string()))?;
    if response.code != "0" {
        return Err(Arc::new(format!(
            "OKX API returned code {}: {}",
            response.code, response.msg
        )));
    }

    Ok(Arc::new(response.data))
}

fn shared_positions_result(result: SharedPositionsResult) -> Result<Arc<Vec<OkxPosition>>> {
    result.map_err(|message| TraderRunError::ExchangeApiMessage {
        message: message.as_ref().clone(),
    })
}

fn okx_client_from_credential(credential: &AccountCredential) -> Result<OkxClient> {
    match credential {
        AccountCredential::OkxApiKeySecretPassphraseV1 {
            api_key,
            api_secret,
            passphrase,
        } => Ok(OkxClient::new(OkxCredential {
            api_key: api_key.clone(),
            api_secret: api_secret.clone(),
            passphrase: passphrase.clone(),
        })),
        _ => Err(TraderRunError::CredentialMismatch),
    }
}
