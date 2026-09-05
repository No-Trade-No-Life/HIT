use auth_mini_axum::{AuthMiniLayer, AuthMiniPrincipal};
use axum::{
    Json, Router,
    body::Body,
    extract::{Extension, Path, State},
    http::{HeaderMap, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::db::{
    Credential, Database, DatabaseError, LinkitSettings, Trader, TraderDraft, TraderUpdate,
};
use crate::engine::{Template, TraderRuntime, template, templates, validate_configuration};

#[derive(Clone, Debug)]
struct AppState {
    database: Database,
    runtime: TraderRuntime,
}

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub fn router(database: Database, runtime: TraderRuntime, auth: AuthMiniLayer) -> Router {
    let state = AppState { database, runtime };
    let private = Router::new()
        .route("/me", get(me))
        .route("/setup", post(setup_root))
        .route(
            "/credentials",
            get(list_credentials).post(create_credential),
        )
        .route(
            "/credentials/{id}",
            get(get_credential)
                .put(update_credential)
                .delete(delete_credential),
        )
        .route("/traders", get(list_traders).post(create_trader))
        .route(
            "/traders/{id}",
            get(get_trader).put(update_trader).delete(delete_trader),
        )
        .route("/traders/{id}/signal-token", post(rotate_signal_token))
        .route("/traders/{id}/runs", get(list_runs))
        .route("/linkit", get(get_linkit).put(put_linkit))
        .route_layer(auth);
    Router::new()
        .route("/api/health", get(health))
        .route("/api/templates", get(list_templates))
        .route("/signal/v1/traders/{id}", patch(external_signal))
        .nest("/api/v1", private)
        .fallback(static_asset)
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .layer(cors())
}

fn cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers(Any)
}

async fn health() -> Json<Value> {
    Json(json!({"status":"ok","service":"hit"}))
}
async fn list_templates() -> Json<Vec<Template>> {
    Json(templates())
}

async fn me(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    Ok(Json(Me {
        user_id: principal.subject.clone(),
        is_root: root_user_id.as_deref() == Some(&principal.subject),
        setup_required: root_user_id.is_none(),
    }))
}

async fn setup_root(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Me>, ApiError> {
    let root_user_id = state.database.root_user_id()?;
    if let Some(root_user_id) = root_user_id {
        if root_user_id != principal.subject {
            return Err(ApiError::forbidden(
                "root user has already been initialized",
            ));
        }
    } else {
        state.database.initialize_root_user(&principal.subject)?;
    }
    Ok(Json(Me {
        user_id: principal.subject,
        is_root: true,
        setup_required: false,
    }))
}

async fn list_credentials(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Vec<Credential>>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let owner_filter = if actor.is_root {
        None
    } else {
        Some(actor.user_id.as_str())
    };
    Ok(Json(state.database.list_credentials(owner_filter)?))
}

async fn get_credential(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<Credential>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let credential = state
        .database
        .get_credential(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&credential.owner_id)?;
    Ok(Json(credential))
}

async fn create_credential(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<CredentialInput>,
) -> Result<(StatusCode, Json<Credential>), ApiError> {
    let secret = credential_secret(&input)?;
    let credential = state.database.create_credential(
        &principal.subject,
        &input.exchange,
        &input.label,
        &secret,
    )?;
    Ok((StatusCode::CREATED, Json(credential)))
}

async fn update_credential(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<CredentialInput>,
) -> Result<Json<Credential>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let existing = state
        .database
        .get_credential(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&existing.owner_id)?;
    if existing.exchange != input.exchange {
        return Err(ApiError::bad_request(
            "credential exchange cannot be changed",
        ));
    }
    let secret = credential_secret(&input)?;
    state
        .database
        .update_credential(&id, &input.label, &secret)?
        .map(Json)
        .ok_or_else(ApiError::not_found)
}

async fn delete_credential(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let credential = state
        .database
        .get_credential(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&credential.owner_id)?;
    state.database.delete_credential(&id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_traders(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Vec<Trader>>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let owner_filter = if actor.is_root {
        None
    } else {
        Some(actor.user_id.as_str())
    };
    Ok(Json(state.database.list_traders(owner_filter)?))
}

async fn get_trader(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    Ok(Json(trader))
}

async fn create_trader(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<TraderInput>,
) -> Result<(StatusCode, Json<CreatedTrader>), ApiError> {
    validate_input(&state.database, &principal.subject, &input)?;
    let (trader, signal_token) = state.database.create_trader(&TraderDraft {
        owner_id: principal.subject,
        name: input.name,
        template_id: input.template_id,
        credential_id: input.credential_id,
        params: input.params,
        signal: input.signal,
        enabled: input.enabled,
    })?;
    state.runtime.reconcile().await;
    Ok((
        StatusCode::CREATED,
        Json(CreatedTrader {
            trader,
            signal_token,
        }),
    ))
}

async fn update_trader(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<TraderUpdateInput>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    let create_input = TraderInput {
        name: input.name.clone(),
        template_id: trader.template_id.clone(),
        credential_id: input.credential_id.clone(),
        params: input.params.clone(),
        signal: input.signal.clone(),
        enabled: input.enabled,
    };
    validate_input(&state.database, &trader.owner_id, &create_input)?;
    let trader = state
        .database
        .update_trader(
            &id,
            &TraderUpdate {
                name: input.name,
                credential_id: input.credential_id,
                params: input.params,
                signal: input.signal,
                enabled: input.enabled,
            },
        )?
        .ok_or_else(ApiError::not_found)?;
    state.runtime.reconcile().await;
    Ok(Json(trader))
}

async fn delete_trader(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    state.database.delete_trader(&id)?;
    state.runtime.reconcile().await;
    Ok(StatusCode::NO_CONTENT)
}

async fn rotate_signal_token(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<SignalToken>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    let token = state
        .database
        .rotate_signal_token(&id)?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(SignalToken {
        signal_token: token,
    }))
}

async fn list_runs(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<Vec<crate::db::Run>>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    Ok(Json(state.database.list_runs(&id)?))
}

async fn get_linkit(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<Option<LinkitSettings>>, ApiError> {
    Ok(Json(state.database.linkit_settings(&principal.subject)?))
}

async fn put_linkit(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Json(input): Json<LinkitInput>,
) -> Result<Json<LinkitSettings>, ApiError> {
    if input.recipient_username.trim().is_empty() || !input.bot_token.starts_with("sk-") {
        return Err(ApiError::bad_request(
            "recipient_username and a Linkit sk- token are required",
        ));
    }
    Ok(Json(state.database.put_linkit_settings(
        &principal.subject,
        input.recipient_username.trim(),
        &input.bot_token,
    )?))
}

async fn external_signal(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(input): Json<ExternalSignal>,
) -> Result<Json<Value>, ApiError> {
    let token =
        bearer_token(&headers).ok_or_else(|| ApiError::unauthorized("missing signal API key"))?;
    if !token.starts_with("sk-") {
        return Err(ApiError::unauthorized("invalid signal API key"));
    }
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    validate_configuration(
        &trader.template_id,
        &trader.credential_id,
        &trader.params,
        &input.signal,
    )
    .map_err(ApiError::bad_request)?;
    if !state.database.update_signal(&id, token, &input.signal)? {
        return Err(ApiError::unauthorized("invalid signal API key"));
    }
    Ok(Json(json!({"trader_id": id, "updated": true})))
}

async fn static_asset(uri: axum::extract::OriginalUri) -> Response {
    let path = uri.0.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    let asset = WebAssets::get(path).or_else(|| WebAssets::get("index.html"));
    let Some(asset) = asset else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    (
        [(header::CONTENT_TYPE, mime.as_ref())],
        Body::from(asset.data.into_owned()),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
struct CredentialInput {
    exchange: String,
    label: String,
    secrets: Value,
}
#[derive(Debug, Deserialize)]
struct TraderInput {
    name: String,
    template_id: String,
    credential_id: String,
    params: Value,
    signal: Value,
    enabled: bool,
}
#[derive(Debug, Deserialize)]
struct TraderUpdateInput {
    name: String,
    credential_id: String,
    params: Value,
    signal: Value,
    enabled: bool,
}
#[derive(Debug, Deserialize)]
struct ExternalSignal {
    signal: Value,
}
#[derive(Debug, Deserialize)]
struct LinkitInput {
    recipient_username: String,
    bot_token: String,
}
#[derive(Debug, Serialize)]
struct Me {
    user_id: String,
    is_root: bool,
    setup_required: bool,
}
#[derive(Debug, Serialize)]
struct CreatedTrader {
    trader: Trader,
    signal_token: String,
}
#[derive(Debug, Serialize)]
struct SignalToken {
    signal_token: String,
}

fn credential_secret(input: &CredentialInput) -> Result<Value, ApiError> {
    if input.label.trim().is_empty() {
        return Err(ApiError::bad_request("credential label is required"));
    }
    let Some(secrets) = input.secrets.as_object() else {
        return Err(ApiError::bad_request(
            "credential secrets must be an object",
        ));
    };
    let field = |name: &str| {
        secrets
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| ApiError::bad_request(format!("credential field {name} is required")))
    };
    match input.exchange.as_str() {
        "binance" => Ok(
            json!({"credential_type":"binance_um_futures.api_key_secret_v1","api_key":field("api_key")?,"api_secret":field("api_secret")?}),
        ),
        "okx" => Ok(
            json!({"credential_type":"okx.api_key_secret_passphrase_v1","api_key":field("api_key")?,"api_secret":field("api_secret")?,"passphrase":field("passphrase")?}),
        ),
        "ctpd" => Ok(
            json!({"credential_type":"ctpd.http_api_key_v1","base_url":field("base_url")?,"api_key":field("api_key")?}),
        ),
        _ => Err(ApiError::bad_request("unsupported exchange")),
    }
}

fn validate_input(
    database: &Database,
    owner_id: &str,
    input: &TraderInput,
) -> Result<(), ApiError> {
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("trader name is required"));
    }
    let template = template(&input.template_id)
        .ok_or_else(|| ApiError::bad_request("unknown trader template"))?;
    let credential = database
        .get_credential(&input.credential_id)?
        .ok_or_else(ApiError::not_found)?;
    if credential.owner_id != owner_id {
        return Err(ApiError::forbidden("credential is owned by another user"));
    }
    if credential.exchange != template.exchange {
        return Err(ApiError::bad_request(
            "credential exchange does not match trader template",
        ));
    }
    validate_configuration(
        &input.template_id,
        &input.credential_id,
        &input.params,
        &input.signal,
    )
    .map_err(ApiError::bad_request)
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

#[derive(Debug)]
struct Actor {
    user_id: String,
    is_root: bool,
}
impl Actor {
    fn from_principal(
        database: &Database,
        principal: &AuthMiniPrincipal,
    ) -> Result<Self, ApiError> {
        Ok(Self {
            user_id: principal.subject.clone(),
            is_root: database.root_user_id()?.as_deref() == Some(&principal.subject),
        })
    }
    fn assert_owner(&self, owner_id: &str) -> Result<(), ApiError> {
        if self.is_root || self.user_id == owner_id {
            Ok(())
        } else {
            Err(ApiError::forbidden("resource belongs to another user"))
        }
    }
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("state storage failed")]
    Database(#[from] DatabaseError),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("not found")]
    NotFound,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest(message.into())
    }
    fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }
    fn unauthorized(message: impl Into<String>) -> Self {
        Self::Unauthorized(message.into())
    }
    fn not_found() -> Self {
        Self::NotFound
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        };
        let message = match &self {
            Self::Database(_) => "internal state error".to_owned(),
            other => other.to_string(),
        };
        (status, Json(json!({"error":message}))).into_response()
    }
}
