use async_trait::async_trait;
use axum::http::{HeaderMap, header};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use reqwest::Client;
use serde::Deserialize;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use url::Url;

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const JWKS_CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminPermission {
    View,
    Operate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminPrincipal {
    subject: String,
    permissions: Vec<String>,
}

impl AdminPrincipal {
    pub fn new(subject: String, mut permissions: Vec<String>) -> Self {
        permissions.sort();
        permissions.dedup();
        Self {
            subject,
            permissions,
        }
    }
    pub fn subject(&self) -> &str {
        &self.subject
    }
    pub fn permissions(&self) -> &[String] {
        &self.permissions
    }
    fn permits(&self, permission: AdminPermission) -> bool {
        match permission {
            AdminPermission::View => self
                .permissions
                .iter()
                .any(|permission| permission == "admin:read" || permission == "admin:operate"),
            AdminPermission::Operate => self
                .permissions
                .iter()
                .any(|permission| permission == "admin:operate"),
        }
    }
}

#[async_trait]
pub trait AdminAuthorizer: Send + Sync {
    async fn authorize(
        &self,
        headers: &HeaderMap,
        permission: AdminPermission,
    ) -> Result<AdminPrincipal, AdminAuthorizationError>;
}

pub struct OidcAdminAuthorizer {
    issuer: Url,
    audience: String,
    jwks_url: Url,
    client: Client,
    keys: RwLock<Option<CachedJwks>>,
    rate_limiter: SubjectRateLimiter,
}

impl OidcAdminAuthorizer {
    pub fn new(
        issuer: Url,
        audience: String,
        jwks_url: Url,
        timeout: Duration,
        requests_per_minute: u32,
        mutations_per_minute: u32,
    ) -> Result<Self, AdminAuthorizationError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|_| AdminAuthorizationError::Unavailable)?;
        Ok(Self {
            issuer,
            audience,
            jwks_url,
            client,
            keys: RwLock::new(None),
            rate_limiter: SubjectRateLimiter::new(requests_per_minute, mutations_per_minute),
        })
    }

    async fn key_for(&self, key_id: &str) -> Result<DecodingKey, AdminAuthorizationError> {
        if let Some(keys) = self.keys.read().await.as_ref()
            && keys.expires_at > Instant::now()
            && let Some(key) = keys
                .value
                .keys
                .iter()
                .find(|key| key.common.key_id.as_deref() == Some(key_id))
        {
            return DecodingKey::from_jwk(key).map_err(|_| AdminAuthorizationError::Unauthorized);
        }
        let keys = self.refresh_keys().await?;
        let key = keys
            .keys
            .iter()
            .find(|key| key.common.key_id.as_deref() == Some(key_id))
            .ok_or(AdminAuthorizationError::Unauthorized)?;
        DecodingKey::from_jwk(key).map_err(|_| AdminAuthorizationError::Unauthorized)
    }

    async fn refresh_keys(&self) -> Result<JwkSet, AdminAuthorizationError> {
        let response = self
            .client
            .get(self.jwks_url.clone())
            .send()
            .await
            .map_err(|_| AdminAuthorizationError::Unavailable)?;
        if !response.status().is_success() {
            return Err(AdminAuthorizationError::Unavailable);
        }
        let keys = response
            .json::<JwkSet>()
            .await
            .map_err(|_| AdminAuthorizationError::Unavailable)?;
        if keys.keys.is_empty() {
            return Err(AdminAuthorizationError::Unavailable);
        }
        *self.keys.write().await = Some(CachedJwks {
            value: keys.clone(),
            expires_at: Instant::now()
                .checked_add(JWKS_CACHE_TTL)
                .unwrap_or_else(Instant::now),
        });
        Ok(keys)
    }

    async fn authenticate(
        &self,
        headers: &HeaderMap,
    ) -> Result<AdminPrincipal, AdminAuthorizationError> {
        let token = bearer_token(headers)?;
        let header = decode_header(token).map_err(|_| AdminAuthorizationError::Unauthorized)?;
        if header.alg != Algorithm::RS256 {
            return Err(AdminAuthorizationError::Unauthorized);
        }
        let key_id = header.kid.ok_or(AdminAuthorizationError::Unauthorized)?;
        let key = self.key_for(&key_id).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        let claims = decode::<OidcClaims>(token, &key, &validation)
            .map_err(|_| AdminAuthorizationError::Unauthorized)?
            .claims;
        if claims.sub.trim().is_empty() {
            return Err(AdminAuthorizationError::Unauthorized);
        }
        Ok(AdminPrincipal::new(claims.sub, claims.permissions))
    }
}

struct CachedJwks {
    value: JwkSet,
    expires_at: Instant,
}

#[async_trait]
impl AdminAuthorizer for OidcAdminAuthorizer {
    async fn authorize(
        &self,
        headers: &HeaderMap,
        permission: AdminPermission,
    ) -> Result<AdminPrincipal, AdminAuthorizationError> {
        let principal = self.authenticate(headers).await?;
        if !principal.permits(permission) {
            return Err(AdminAuthorizationError::Forbidden);
        }
        self.rate_limiter
            .check(
                principal.subject(),
                matches!(permission, AdminPermission::Operate),
            )
            .await?;
        Ok(principal)
    }
}

#[derive(Deserialize)]
struct OidcClaims {
    sub: String,
    #[serde(default)]
    permissions: Vec<String>,
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, AdminAuthorizationError> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .ok_or(AdminAuthorizationError::Unauthorized)
}

#[derive(Debug, Error)]
pub enum AdminAuthorizationError {
    #[error("administrator authentication failed")]
    Unauthorized,
    #[error("administrator permission denied")]
    Forbidden,
    #[error("administrator request rate limited")]
    RateLimited { retry_after_seconds: u64 },
    #[error("administrator identity provider unavailable")]
    Unavailable,
}

struct SubjectRateLimiter {
    reads_per_minute: usize,
    mutations_per_minute: usize,
    accepted_at: Mutex<HashMap<String, VecDeque<Instant>>>,
}
impl SubjectRateLimiter {
    fn new(reads_per_minute: u32, mutations_per_minute: u32) -> Self {
        Self {
            reads_per_minute: reads_per_minute as usize,
            mutations_per_minute: mutations_per_minute as usize,
            accepted_at: Mutex::new(HashMap::new()),
        }
    }
    async fn check(&self, subject: &str, mutation: bool) -> Result<(), AdminAuthorizationError> {
        let limit = if mutation {
            self.mutations_per_minute
        } else {
            self.reads_per_minute
        };
        let now = Instant::now();
        let mut all = self.accepted_at.lock().await;
        let accepted = all
            .entry(format!(
                "{}:{}",
                if mutation { "mutation" } else { "read" },
                subject
            ))
            .or_default();
        while accepted
            .front()
            .is_some_and(|at| now.duration_since(*at) >= RATE_LIMIT_WINDOW)
        {
            accepted.pop_front();
        }
        if accepted.len() >= limit {
            return Err(AdminAuthorizationError::RateLimited {
                retry_after_seconds: accepted.front().map_or(1, |at| {
                    seconds_ceil(
                        at.checked_add(RATE_LIMIT_WINDOW)
                            .unwrap_or(now)
                            .saturating_duration_since(now),
                    )
                }),
            });
        }
        accepted.push_back(now);
        Ok(())
    }
}

fn seconds_ceil(duration: Duration) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() > 0))
        .max(1)
}

#[cfg(test)]
pub struct TestAdminAuthorizer;
#[cfg(test)]
#[async_trait]
impl AdminAuthorizer for TestAdminAuthorizer {
    async fn authorize(
        &self,
        headers: &HeaderMap,
        permission: AdminPermission,
    ) -> Result<AdminPrincipal, AdminAuthorizationError> {
        let token = bearer_token(headers)?;
        let principal = match token {
            "viewer-token" => {
                AdminPrincipal::new("viewer".to_owned(), vec!["admin:read".to_owned()])
            }
            "operator-token" => {
                AdminPrincipal::new("operator".to_owned(), vec!["admin:operate".to_owned()])
            }
            _ => return Err(AdminAuthorizationError::Unauthorized),
        };
        if principal.permits(permission) {
            Ok(principal)
        } else {
            Err(AdminAuthorizationError::Forbidden)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operate_permission_includes_read_permission() {
        let principal = AdminPrincipal::new(
            "operator".to_owned(),
            vec!["admin:operate".to_owned(), "admin:operate".to_owned()],
        );

        assert!(principal.permits(AdminPermission::View));
        assert!(principal.permits(AdminPermission::Operate));
        assert_eq!(principal.permissions(), ["admin:operate"]);
    }

    #[tokio::test]
    async fn rate_limits_reads_and_mutations_independently_per_subject() {
        let limiter = SubjectRateLimiter::new(1, 1);

        assert!(limiter.check("subject-a", false).await.is_ok());
        assert!(matches!(
            limiter.check("subject-a", false).await,
            Err(AdminAuthorizationError::RateLimited { .. })
        ));
        assert!(limiter.check("subject-a", true).await.is_ok());
        assert!(limiter.check("subject-b", false).await.is_ok());
    }
}
