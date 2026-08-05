#![forbid(unsafe_code)]

use async_trait::async_trait;
use secrecy::SecretString;
use serde::Serialize;
use std::{collections::BTreeMap, env, net::SocketAddr, time::Duration};
use thiserror::Error;
use url::Url;

#[derive(Clone, Debug)]
pub struct AppConfig {
    bind_address: SocketAddr,
    database_url: SecretString,
    redis_url: SecretString,
    meilisearch_url: Url,
    meilisearch_master_key: SecretString,
    admin_oidc_issuer: Url,
    admin_oidc_audience: String,
    admin_oidc_jwks_url: Url,
    admin_rate_limit_per_minute: u32,
    admin_mutation_rate_limit_per_minute: u32,
    log_filter: String,
    dependency_check_timeout: Duration,
    public_stale_after_days: u16,
    public_refresh_cooldown_hours: u16,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_pairs(env::vars())
    }

    pub fn from_pairs<I>(pairs: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let values = pairs.into_iter().collect::<BTreeMap<_, _>>();
        let bind_address = values
            .get("API_BIND_ADDRESS")
            .filter(|value| !value.trim().is_empty())
            .map_or(Ok("0.0.0.0:3000".to_owned()), |value| Ok(value.to_owned()))?
            .parse::<SocketAddr>()
            .map_err(|_| ConfigError::invalid("API_BIND_ADDRESS"))?;

        let database_url = required_url(&values, "DATABASE_URL")?;
        let redis_url = required_url(&values, "REDIS_URL")?;
        let meilisearch_url = required_url(&values, "MEILISEARCH_URL")?;
        let meilisearch_master_key = required(&values, "MEILI_MASTER_KEY")?;
        let admin_oidc_issuer = required_url(&values, "ADMIN_OIDC_ISSUER")?;
        let admin_oidc_audience = required(&values, "ADMIN_OIDC_AUDIENCE")?;
        let admin_oidc_jwks_url = required_url(&values, "ADMIN_OIDC_JWKS_URL")?;
        let admin_rate_limit_per_minute = values
            .get("ADMIN_RATE_LIMIT_PER_MINUTE")
            .filter(|value| !value.trim().is_empty())
            .map_or(Ok(60_u32), |value| {
                value
                    .parse::<u32>()
                    .map_err(|_| ConfigError::invalid("ADMIN_RATE_LIMIT_PER_MINUTE"))
            })
            .and_then(|limit| {
                if limit == 0 {
                    Err(ConfigError::invalid("ADMIN_RATE_LIMIT_PER_MINUTE"))
                } else {
                    Ok(limit)
                }
            })?;
        let admin_mutation_rate_limit_per_minute =
            positive_u32(&values, "ADMIN_MUTATION_RATE_LIMIT_PER_MINUTE", 30)?;
        let dependency_check_timeout = values
            .get("DEPENDENCY_CHECK_TIMEOUT_MS")
            .filter(|value| !value.trim().is_empty())
            .map_or(Ok(2_000_u64), |value| {
                value
                    .parse::<u64>()
                    .map_err(|_| ConfigError::invalid("DEPENDENCY_CHECK_TIMEOUT_MS"))
            })
            .and_then(|milliseconds| {
                if milliseconds == 0 {
                    Err(ConfigError::invalid("DEPENDENCY_CHECK_TIMEOUT_MS"))
                } else {
                    Ok(Duration::from_millis(milliseconds))
                }
            })?;
        let public_stale_after_days = values
            .get("PUBLIC_STALE_AFTER_DAYS")
            .filter(|value| !value.trim().is_empty())
            .map_or(Ok(30_u16), |value| {
                value
                    .parse::<u16>()
                    .map_err(|_| ConfigError::invalid("PUBLIC_STALE_AFTER_DAYS"))
            })
            .and_then(|days| {
                if days == 0 {
                    Err(ConfigError::invalid("PUBLIC_STALE_AFTER_DAYS"))
                } else {
                    Ok(days)
                }
            })?;
        let public_refresh_cooldown_hours =
            positive_u16(&values, "PUBLIC_REFRESH_COOLDOWN_HOURS", 24)?;

        Ok(Self {
            bind_address,
            database_url: SecretString::from(database_url),
            redis_url: SecretString::from(redis_url),
            meilisearch_url: Url::parse(&meilisearch_url)
                .map_err(|_| ConfigError::invalid("MEILISEARCH_URL"))?,
            meilisearch_master_key: SecretString::from(meilisearch_master_key),
            admin_oidc_issuer: Url::parse(&admin_oidc_issuer)
                .map_err(|_| ConfigError::invalid("ADMIN_OIDC_ISSUER"))?,
            admin_oidc_audience,
            admin_oidc_jwks_url: Url::parse(&admin_oidc_jwks_url)
                .map_err(|_| ConfigError::invalid("ADMIN_OIDC_JWKS_URL"))?,
            admin_rate_limit_per_minute,
            admin_mutation_rate_limit_per_minute,
            log_filter: values
                .get("RUST_LOG")
                .filter(|value| !value.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| "techatlas_api=info,tower_http=info".to_owned()),
            dependency_check_timeout,
            public_stale_after_days,
            public_refresh_cooldown_hours,
        })
    }

    pub fn bind_address(&self) -> SocketAddr {
        self.bind_address
    }

    pub fn database_url(&self) -> &SecretString {
        &self.database_url
    }

    pub fn redis_url(&self) -> &SecretString {
        &self.redis_url
    }

    pub fn meilisearch_url(&self) -> &Url {
        &self.meilisearch_url
    }

    pub fn meilisearch_master_key(&self) -> &SecretString {
        &self.meilisearch_master_key
    }

    pub fn admin_oidc_issuer(&self) -> &Url {
        &self.admin_oidc_issuer
    }
    pub fn admin_oidc_audience(&self) -> &str {
        &self.admin_oidc_audience
    }
    pub fn admin_oidc_jwks_url(&self) -> &Url {
        &self.admin_oidc_jwks_url
    }

    pub fn admin_rate_limit_per_minute(&self) -> u32 {
        self.admin_rate_limit_per_minute
    }
    pub fn admin_mutation_rate_limit_per_minute(&self) -> u32 {
        self.admin_mutation_rate_limit_per_minute
    }

    pub fn log_filter(&self) -> &str {
        &self.log_filter
    }

    pub fn dependency_check_timeout(&self) -> Duration {
        self.dependency_check_timeout
    }
    pub fn public_stale_after_days(&self) -> u16 {
        self.public_stale_after_days
    }
    pub fn public_refresh_cooldown_hours(&self) -> u16 {
        self.public_refresh_cooldown_hours
    }
}

fn positive_u16(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u16,
) -> Result<u16, ConfigError> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map_or(Ok(default), |value| {
            value.parse().map_err(|_| ConfigError::invalid(name))
        })
        .and_then(|value| {
            if value == 0 {
                Err(ConfigError::invalid(name))
            } else {
                Ok(value)
            }
        })
}

fn positive_u32(
    values: &BTreeMap<String, String>,
    name: &'static str,
    default: u32,
) -> Result<u32, ConfigError> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .map_or(Ok(default), |value| {
            value.parse().map_err(|_| ConfigError::invalid(name))
        })
        .and_then(|value| {
            if value == 0 {
                Err(ConfigError::invalid(name))
            } else {
                Ok(value)
            }
        })
}

fn required(values: &BTreeMap<String, String>, name: &'static str) -> Result<String, ConfigError> {
    values
        .get(name)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or(ConfigError::Missing { name })
}

fn required_url(
    values: &BTreeMap<String, String>,
    name: &'static str,
) -> Result<String, ConfigError> {
    let value = required(values, name)?;
    Url::parse(&value).map_err(|_| ConfigError::invalid(name))?;
    Ok(value)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("missing required configuration: {name}")]
    Missing { name: &'static str },
    #[error("invalid configuration: {name}")]
    Invalid { name: &'static str },
}

impl ConfigError {
    fn invalid(name: &'static str) -> Self {
        Self::Invalid { name }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyStatus {
    Ready,
    Unavailable,
}

impl DependencyStatus {
    pub fn is_ready(self) -> bool {
        matches!(self, Self::Ready)
    }
}

#[async_trait]
pub trait DependencyProbe: Send + Sync {
    async fn check(&self) -> DependencyStatus;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_pairs() -> Vec<(String, String)> {
        vec![
            (
                "DATABASE_URL".to_owned(),
                "postgres://user:password@localhost/database".to_owned(),
            ),
            ("REDIS_URL".to_owned(), "redis://localhost:6379".to_owned()),
            (
                "MEILISEARCH_URL".to_owned(),
                "http://localhost:7700".to_owned(),
            ),
            ("MEILI_MASTER_KEY".to_owned(), "development-key".to_owned()),
            (
                "ADMIN_OIDC_ISSUER".to_owned(),
                "https://issuer.example.test".to_owned(),
            ),
            ("ADMIN_OIDC_AUDIENCE".to_owned(), "techatlas-api".to_owned()),
            (
                "ADMIN_OIDC_JWKS_URL".to_owned(),
                "https://issuer.example.test/keys".to_owned(),
            ),
        ]
    }

    #[test]
    fn uses_safe_defaults_for_optional_settings() {
        let config =
            AppConfig::from_pairs(valid_pairs()).expect("valid configuration should parse");

        assert_eq!(config.bind_address().to_string(), "0.0.0.0:3000");
        assert_eq!(config.log_filter(), "techatlas_api=info,tower_http=info");
        assert_eq!(config.dependency_check_timeout(), Duration::from_secs(2));
        assert_eq!(config.admin_rate_limit_per_minute(), 60);
        assert_eq!(config.admin_mutation_rate_limit_per_minute(), 30);
        assert_eq!(config.public_stale_after_days(), 30);
        assert_eq!(config.public_refresh_cooldown_hours(), 24);
    }

    #[test]
    fn rejects_missing_required_settings_without_exposing_values() {
        let pairs = valid_pairs()
            .into_iter()
            .filter(|(name, _)| name != "DATABASE_URL")
            .collect::<Vec<_>>();

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Missing {
                name: "DATABASE_URL"
            }
        );
    }

    #[test]
    fn rejects_invalid_bind_addresses() {
        let mut pairs = valid_pairs();
        pairs.push(("API_BIND_ADDRESS".to_owned(), "not-an-address".to_owned()));

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Invalid {
                name: "API_BIND_ADDRESS"
            }
        );
    }

    #[test]
    fn rejects_invalid_service_urls() {
        let mut pairs = valid_pairs();
        pairs.retain(|(name, _)| name != "REDIS_URL");
        pairs.push(("REDIS_URL".to_owned(), "not a url".to_owned()));

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Invalid { name: "REDIS_URL" }
        );
    }

    #[test]
    fn rejects_zero_dependency_check_timeout() {
        let mut pairs = valid_pairs();
        pairs.push(("DEPENDENCY_CHECK_TIMEOUT_MS".to_owned(), "0".to_owned()));

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Invalid {
                name: "DEPENDENCY_CHECK_TIMEOUT_MS"
            }
        );
    }

    #[test]
    fn rejects_zero_admin_rate_limit() {
        let mut pairs = valid_pairs();
        pairs.push(("ADMIN_RATE_LIMIT_PER_MINUTE".to_owned(), "0".to_owned()));

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Invalid {
                name: "ADMIN_RATE_LIMIT_PER_MINUTE"
            }
        );
    }

    #[test]
    fn rejects_zero_public_refresh_cooldown() {
        let mut pairs = valid_pairs();
        pairs.push(("PUBLIC_REFRESH_COOLDOWN_HOURS".to_owned(), "0".to_owned()));

        assert_eq!(
            AppConfig::from_pairs(pairs).expect_err("configuration should be rejected"),
            ConfigError::Invalid {
                name: "PUBLIC_REFRESH_COOLDOWN_HOURS"
            }
        );
    }
}
