use crate::{
    admin_auth::{AdminAuthorizationError, AdminAuthorizer, AdminPermission},
    app::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use techatlas_models::{CanonicalDomain, Domain, DomainServiceError};
use utoipa::{IntoParams, ToSchema};

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/domains",
            post(create_domain).get(list_domains),
        )
        .route("/api/v1/admin/domains/{canonical_domain}", get(get_domain))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDomainRequest {
    pub domain: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainResponse {
    pub id: String,
    pub canonical_domain: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct DomainListQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DomainListResponse {
    pub domains: Vec<DomainResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: &'static str,
    pub message: &'static str,
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/domains",
    tag = "admin-domains",
    request_body = CreateDomainRequest,
    responses(
        (status = 201, description = "Domain created", body = DomainResponse,
            headers(("Location" = String, description = "Canonical resource location"))),
        (status = 401, description = "Authentication failed", body = ErrorEnvelope,
            headers(("WWW-Authenticate" = String, description = "Bearer authentication challenge"))),
        (status = 409, description = "Domain already exists", body = ErrorEnvelope),
        (status = 422, description = "Request validation failed", body = ErrorEnvelope),
        (status = 429, description = "Rate limit exceeded", body = ErrorEnvelope,
            headers(("Retry-After" = u64, description = "Seconds before retrying"))),
        (status = 503, description = "Service unavailable", body = ErrorEnvelope)
    ),
    security(("oidc_bearer" = []))
)]
pub async fn create_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateDomainRequest>, JsonRejection>,
) -> Result<Response, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    let domain = state
        .admin_domains
        .create_manual(&payload.domain, principal.subject())
        .await
        .map_err(AdminApiError::from_domain_service)?;
    let location = HeaderValue::try_from(format!(
        "/api/v1/admin/domains/{}",
        domain.canonical_domain().as_str()
    ))
    .map_err(|_| AdminApiError::Unavailable)?;
    let mut response = (StatusCode::CREATED, Json(DomainResponse::from(&domain))).into_response();
    response.headers_mut().insert(header::LOCATION, location);

    Ok(response)
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/domains",
    tag = "admin-domains",
    params(DomainListQuery),
    responses(
        (status = 200, description = "Page of active domains", body = DomainListResponse),
        (status = 401, description = "Authentication failed", body = ErrorEnvelope,
            headers(("WWW-Authenticate" = String, description = "Bearer authentication challenge"))),
        (status = 422, description = "Query validation failed", body = ErrorEnvelope),
        (status = 429, description = "Rate limit exceeded", body = ErrorEnvelope,
            headers(("Retry-After" = u64, description = "Seconds before retrying"))),
        (status = 503, description = "Service unavailable", body = ErrorEnvelope)
    ),
    security(("oidc_bearer" = []))
)]
pub async fn list_domains(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<DomainListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<DomainListResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let Query(query) = query.map_err(|_| AdminApiError::Validation)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(AdminApiError::Validation);
    }
    let cursor = query
        .cursor
        .map(|cursor| CanonicalDomain::parse(&cursor))
        .transpose()
        .map_err(|_| AdminApiError::Validation)?;
    let page = state
        .admin_domains
        .list_active(cursor.as_ref(), limit)
        .await
        .map_err(AdminApiError::from_domain_service)?;

    Ok(Json(DomainListResponse {
        domains: page.domains().iter().map(DomainResponse::from).collect(),
        next_cursor: page.next_cursor().map(|cursor| cursor.as_str().to_owned()),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/domains/{canonical_domain}",
    tag = "admin-domains",
    params(("canonical_domain" = String, Path, description = "Domain name or normalizable HTTP(S) URL")),
    responses(
        (status = 200, description = "Active domain", body = DomainResponse),
        (status = 401, description = "Authentication failed", body = ErrorEnvelope,
            headers(("WWW-Authenticate" = String, description = "Bearer authentication challenge"))),
        (status = 404, description = "Domain not found", body = ErrorEnvelope),
        (status = 422, description = "Path validation failed", body = ErrorEnvelope),
        (status = 429, description = "Rate limit exceeded", body = ErrorEnvelope,
            headers(("Retry-After" = u64, description = "Seconds before retrying"))),
        (status = 503, description = "Service unavailable", body = ErrorEnvelope)
    ),
    security(("oidc_bearer" = []))
)]
pub async fn get_domain(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(canonical_domain): Path<String>,
) -> Result<Json<DomainResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let domain = state
        .admin_domains
        .find_active(&canonical_domain)
        .await
        .map_err(AdminApiError::from_domain_service)?
        .ok_or(AdminApiError::NotFound)?;

    Ok(Json(DomainResponse::from(&domain)))
}

impl From<&Domain> for DomainResponse {
    fn from(domain: &Domain) -> Self {
        Self {
            id: domain.id().as_uuid().to_string(),
            canonical_domain: domain.canonical_domain().as_str().to_owned(),
        }
    }
}

pub(crate) async fn authorize(
    authorizer: &dyn AdminAuthorizer,
    headers: &HeaderMap,
    permission: AdminPermission,
) -> Result<crate::admin_auth::AdminPrincipal, AdminApiError> {
    authorizer
        .authorize(headers, permission)
        .await
        .map_err(Into::into)
}

pub enum AdminApiError {
    Unauthorized,
    Forbidden,
    Validation,
    Duplicate,
    Conflict,
    NotFound,
    RateLimited { retry_after_seconds: u64 },
    Unavailable,
}

impl AdminApiError {
    fn from_domain_service(error: DomainServiceError) -> Self {
        match error {
            DomainServiceError::InvalidInput(_) => Self::Validation,
            DomainServiceError::Duplicate => Self::Duplicate,
            DomainServiceError::Unavailable => Self::Unavailable,
        }
    }
}

impl From<AdminAuthorizationError> for AdminApiError {
    fn from(error: AdminAuthorizationError) -> Self {
        match error {
            AdminAuthorizationError::Unauthorized => Self::Unauthorized,
            AdminAuthorizationError::Forbidden => Self::Forbidden,
            AdminAuthorizationError::RateLimited {
                retry_after_seconds,
            } => Self::RateLimited {
                retry_after_seconds,
            },
            AdminAuthorizationError::Unavailable => Self::Unavailable,
        }
    }
}

impl IntoResponse for AdminApiError {
    fn into_response(self) -> Response {
        let (status, code, message, retry_after) = match self {
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "a valid bearer token is required",
                None,
            ),
            Self::Validation => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "request input is invalid",
                None,
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "the authenticated principal lacks the required admin role",
                None,
            ),
            Self::Duplicate => (
                StatusCode::CONFLICT,
                "domain_conflict",
                "an active domain with this canonical identity already exists",
                None,
            ),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "operation_conflict",
                "the operation is not eligible in the current state",
                None,
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "domain_not_found",
                "the active domain was not found",
                None,
            ),
            Self::RateLimited {
                retry_after_seconds,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "the admin request rate limit has been exceeded",
                Some(retry_after_seconds),
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "the domain service is unavailable",
                None,
            ),
        };
        let mut response = (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response();
        if status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        if let Some(retry_after) = retry_after
            && let Ok(retry_after) = HeaderValue::try_from(retry_after.to_string())
        {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, retry_after);
        }
        response
    }
}
