use std::sync::Arc;

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
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::db::{
    Credential, Database, DatabaseError, LinkitSettings, SignalHistory, Trader, TraderDraft,
    TraderUpdate,
};
use crate::engine::{Template, TraderRuntime, template, templates, validate_configuration};
use crate::resources::{ResourceMonitor, SystemResourcesSnapshot};

#[derive(Clone)]
struct AppState {
    database: Database,
    runtime: TraderRuntime,
    resources: Arc<Mutex<ResourceMonitor>>,
}

#[derive(RustEmbed)]
#[folder = "web/dist/"]
struct WebAssets;

pub fn router(database: Database, runtime: TraderRuntime, auth: AuthMiniLayer) -> Router {
    let resources = Arc::new(Mutex::new(ResourceMonitor::new(database.database_path())));
    let state = AppState {
        database,
        runtime,
        resources,
    };
    let private = Router::new()
        .route("/me", get(me))
        .route("/setup", post(setup_root))
        .route("/system/resources", get(system_resources))
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
        .route("/traders/{id}/enabled", patch(set_trader_enabled))
        .route("/traders/{id}/name", patch(set_trader_name))
        .route("/traders/{id}/params", patch(set_trader_params))
        .route("/traders/{id}/signal", patch(set_trader_signal))
        .route("/traders/{id}/signal-history", get(list_signal_history))
        .route("/traders/{id}/signal-token", post(rotate_signal_token))
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

async fn system_resources(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
) -> Result<Json<SystemResourcesSnapshot>, ApiError> {
    Actor::from_principal(&state.database, &principal)?.assert_root()?;
    Ok(Json(state.resources.lock().await.sample()?))
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

async fn set_trader_enabled(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<EnabledInput>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    let trader = state
        .database
        .set_trader_enabled(&id, input.enabled)?
        .ok_or_else(ApiError::not_found)?;
    state.runtime.reconcile().await;
    Ok(Json(trader))
}

async fn set_trader_name(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<NameInput>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    validate_trader_name(&input.name)?;
    let trader = state
        .database
        .set_trader_name(&id, &input.name)?
        .ok_or_else(ApiError::not_found)?;
    Ok(Json(trader))
}

async fn set_trader_params(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<ParamsInput>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    validate_configuration(
        &trader.template_id,
        &trader.credential_id,
        &input.params,
        &trader.signal,
    )
    .map_err(ApiError::bad_request)?;
    let trader = state
        .database
        .set_trader_params(&id, &input.params)?
        .ok_or_else(ApiError::not_found)?;
    state.runtime.reconcile().await;
    Ok(Json(trader))
}

async fn set_trader_signal(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
    Json(input): Json<SignalInput>,
) -> Result<Json<Trader>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    validate_configuration(
        &trader.template_id,
        &trader.credential_id,
        &trader.params,
        &input.signal,
    )
    .map_err(ApiError::bad_request)?;
    let trader = state
        .database
        .set_trader_signal(&id, &input.signal)?
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

async fn list_signal_history(
    State(state): State<AppState>,
    Extension(principal): Extension<AuthMiniPrincipal>,
    Path(id): Path<String>,
) -> Result<Json<Vec<SignalHistory>>, ApiError> {
    let actor = Actor::from_principal(&state.database, &principal)?;
    let trader = state
        .database
        .get_trader(&id)?
        .ok_or_else(ApiError::not_found)?;
    actor.assert_owner(&trader.owner_id)?;
    Ok(Json(state.database.list_signal_history(&id)?))
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
    Json(input): Json<SignalInput>,
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
struct EnabledInput {
    enabled: bool,
}
#[derive(Debug, Deserialize)]
struct NameInput {
    name: String,
}
#[derive(Debug, Deserialize)]
struct ParamsInput {
    params: Value,
}
#[derive(Debug, Deserialize)]
struct SignalInput {
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
    validate_trader_name(&input.name)?;
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

fn validate_trader_name(name: &str) -> Result<(), ApiError> {
    if name.trim().is_empty() {
        return Err(ApiError::bad_request("trader name is required"));
    }
    Ok(())
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
    fn assert_root(&self) -> Result<(), ApiError> {
        if self.is_root {
            Ok(())
        } else {
            Err(ApiError::forbidden("root user is required"))
        }
    }
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("state storage failed")]
    Database(#[from] DatabaseError),
    #[error("resource sampling failed")]
    Resources(#[from] std::io::Error),
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
            Self::Database(_) | Self::Resources(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
        };
        let message = match &self {
            Self::Database(_) | Self::Resources(_) => "internal state error".to_owned(),
            other => other.to_string(),
        };
        (status, Json(json!({"error":message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_resources_require_root_actor() {
        let root = Actor {
            user_id: "root".into(),
            is_root: true,
        };
        let user = Actor {
            user_id: "user".into(),
            is_root: false,
        };

        assert!(root.assert_root().is_ok());
        assert!(matches!(user.assert_root(), Err(ApiError::Forbidden(_))));
    }

    #[test]
    fn trader_name_must_not_be_blank() {
        assert!(validate_trader_name("  ").is_err());
        assert!(validate_trader_name("alpha").is_ok());
    }

    #[tokio::test]
    async fn template_endpoint_returns_complete_schema_metadata() {
        let Json(templates) = list_templates().await;

        assert_eq!(templates.len(), 10);
        for template in templates {
            let document = serde_json::to_value(template).expect("template serializes");
            for field in [
                "id",
                "name",
                "credential_type",
                "description",
                "params_schema",
                "signal_schema",
            ] {
                assert!(
                    document.get(field).is_some_and(Value::is_string)
                        || document[field].is_object()
                );
            }
            for schema_name in ["params_schema", "signal_schema"] {
                let properties = document[schema_name]["properties"]
                    .as_object()
                    .expect("schema properties");
                assert!(!properties.is_empty());
                for property in properties.values() {
                    assert!(
                        property["title"]
                            .as_str()
                            .is_some_and(|title| !title.is_empty())
                    );
                    assert!(
                        property["description"]
                            .as_str()
                            .is_some_and(|description| !description.is_empty())
                    );
                }
            }
        }
    }
}
