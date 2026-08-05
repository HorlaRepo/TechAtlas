use crate::{CanonicalDomain, CrawlFailure, CrawlJobV1, SchedulerSettings};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;

/// The target returned after a worker safely claims a scheduler-owned crawl attempt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClaimedCrawlAttempt {
    Ready { canonical_domain: CanonicalDomain },
    Terminal,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActiveDetectionRule {
    pub technology_slug: String,
    pub technology_category_slug: String,
    pub rule_slug: String,
    pub version: u16,
    pub definition: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlRedirectMetadata {
    from_url: String,
    to_url: String,
    status: u16,
}

impl CrawlRedirectMetadata {
    pub fn new(
        from_url: &str,
        to_url: &str,
        status: u16,
    ) -> Result<Self, CrawlSnapshotMetadataError> {
        let from_url = required_url("redirect source URL", from_url)?;
        let to_url = required_url("redirect destination URL", to_url)?;
        validate_status(status)?;
        Ok(Self {
            from_url,
            to_url,
            status,
        })
    }

    pub fn from_url(&self) -> &str {
        &self.from_url
    }
    pub fn to_url(&self) -> &str {
        &self.to_url
    }
    pub fn status(&self) -> u16 {
        self.status
    }
}

/// Redacted metadata for one successfully acquired response. Raw artifact storage begins in BE-10.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlSnapshotMetadata {
    requested_url: String,
    final_url: String,
    redirects: Vec<CrawlRedirectMetadata>,
    response_status: u16,
    response_headers: BTreeMap<String, String>,
    body_size_bytes: u64,
    captured_at: OffsetDateTime,
    country_observation: Option<CountryObservation>,
}

/// Country evidence derived locally from the public IPs used for a successful crawl.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountryObservation {
    country_code: String,
    source: String,
    source_version: String,
}

impl CountryObservation {
    pub fn new(
        country_code: &str,
        source: &str,
        source_version: &str,
    ) -> Result<Self, CountryObservationError> {
        let country_code = country_code.trim();
        if country_code.len() != 2 || !country_code.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(CountryObservationError::InvalidCountryCode);
        }
        let source =
            required_country_metadata(source).ok_or(CountryObservationError::InvalidSource)?;
        let source_version = required_country_metadata(source_version)
            .ok_or(CountryObservationError::InvalidSourceVersion)?;
        Ok(Self {
            country_code: country_code.to_owned(),
            source,
            source_version,
        })
    }

    pub fn country_code(&self) -> &str {
        &self.country_code
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn source_version(&self) -> &str {
        &self.source_version
    }
}

fn required_country_metadata(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value.len() <= 128).then(|| value.to_owned())
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CountryObservationError {
    #[error("country code must be an uppercase ISO 3166-1 alpha-2 code")]
    InvalidCountryCode,
    #[error("country observation source must be present")]
    InvalidSource,
    #[error("country observation source version must be present")]
    InvalidSourceVersion,
}

/// Why an optional DNS or TLS observation was not available for an otherwise successful crawl.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrawlObservationUnavailableReason {
    NotCaptured,
    NotApplicable,
    Timeout,
    ConnectionFailed,
    Invalid,
}

impl CrawlObservationUnavailableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotCaptured => "not_captured",
            Self::NotApplicable => "not_applicable",
            Self::Timeout => "timeout",
            Self::ConnectionFailed => "connection_failed",
            Self::Invalid => "invalid",
        }
    }
}

/// Immutable DNS facts obtained while safely acquiring a crawl response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlDnsObservation {
    source: String,
    observed_at: OffsetDateTime,
    queried_name: Option<String>,
    addresses: Vec<std::net::IpAddr>,
    unavailable_reason: Option<CrawlObservationUnavailableReason>,
}

impl CrawlDnsObservation {
    pub fn available(
        source: &str,
        observed_at: OffsetDateTime,
        queried_name: &str,
        addresses: Vec<std::net::IpAddr>,
    ) -> Result<Self, CrawlObservationError> {
        Ok(Self {
            source: required_observation_source(source)?,
            observed_at,
            queried_name: required_observation_value("DNS queried name", queried_name)?,
            addresses,
            unavailable_reason: None,
        })
    }

    pub fn unavailable(
        source: &str,
        observed_at: OffsetDateTime,
        reason: CrawlObservationUnavailableReason,
    ) -> Result<Self, CrawlObservationError> {
        Ok(Self {
            source: required_observation_source(source)?,
            observed_at,
            queried_name: None,
            addresses: Vec::new(),
            unavailable_reason: Some(reason),
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn observed_at(&self) -> OffsetDateTime {
        self.observed_at
    }
    pub fn queried_name(&self) -> Option<&str> {
        self.queried_name.as_deref()
    }
    pub fn addresses(&self) -> &[std::net::IpAddr] {
        &self.addresses
    }
    pub fn unavailable_reason(&self) -> Option<CrawlObservationUnavailableReason> {
        self.unavailable_reason
    }
}

/// Whether a TLS certificate chain was verified before its facts were recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsCertificateValidation {
    Verified,
    Failed,
}

impl TlsCertificateValidation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Failed => "validation_failed",
        }
    }
}

/// Immutable, sanitized TLS facts obtained from the final HTTPS endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlTlsObservation {
    source: String,
    observed_at: OffsetDateTime,
    unavailable_reason: Option<CrawlObservationUnavailableReason>,
    validation: Option<TlsCertificateValidation>,
    protocol: Option<String>,
    cipher_suite: Option<String>,
    certificate_subject: Option<String>,
    certificate_issuer: Option<String>,
    subject_alternative_names: Vec<String>,
    certificate_not_before: Option<OffsetDateTime>,
    certificate_not_after: Option<OffsetDateTime>,
}

impl CrawlTlsObservation {
    #[allow(clippy::too_many_arguments)]
    pub fn available(
        source: &str,
        observed_at: OffsetDateTime,
        validation: TlsCertificateValidation,
        protocol: Option<String>,
        cipher_suite: Option<String>,
        certificate_subject: Option<String>,
        certificate_issuer: Option<String>,
        subject_alternative_names: Vec<String>,
        certificate_not_before: Option<OffsetDateTime>,
        certificate_not_after: Option<OffsetDateTime>,
    ) -> Result<Self, CrawlObservationError> {
        Ok(Self {
            source: required_observation_source(source)?,
            observed_at,
            unavailable_reason: None,
            validation: Some(validation),
            protocol: bounded_optional(protocol)?,
            cipher_suite: bounded_optional(cipher_suite)?,
            certificate_subject: bounded_optional(certificate_subject)?,
            certificate_issuer: bounded_optional(certificate_issuer)?,
            subject_alternative_names: subject_alternative_names
                .into_iter()
                .map(|value| bounded_optional(Some(value)))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect(),
            certificate_not_before,
            certificate_not_after,
        })
    }

    pub fn unavailable(
        source: &str,
        observed_at: OffsetDateTime,
        reason: CrawlObservationUnavailableReason,
    ) -> Result<Self, CrawlObservationError> {
        Ok(Self {
            source: required_observation_source(source)?,
            observed_at,
            unavailable_reason: Some(reason),
            validation: None,
            protocol: None,
            cipher_suite: None,
            certificate_subject: None,
            certificate_issuer: None,
            subject_alternative_names: Vec::new(),
            certificate_not_before: None,
            certificate_not_after: None,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn observed_at(&self) -> OffsetDateTime {
        self.observed_at
    }
    pub fn unavailable_reason(&self) -> Option<CrawlObservationUnavailableReason> {
        self.unavailable_reason
    }
    pub fn validation(&self) -> Option<TlsCertificateValidation> {
        self.validation
    }
    pub fn protocol(&self) -> Option<&str> {
        self.protocol.as_deref()
    }
    pub fn cipher_suite(&self) -> Option<&str> {
        self.cipher_suite.as_deref()
    }
    pub fn certificate_subject(&self) -> Option<&str> {
        self.certificate_subject.as_deref()
    }
    pub fn certificate_issuer(&self) -> Option<&str> {
        self.certificate_issuer.as_deref()
    }
    pub fn subject_alternative_names(&self) -> &[String] {
        &self.subject_alternative_names
    }
    pub fn certificate_not_before(&self) -> Option<OffsetDateTime> {
        self.certificate_not_before
    }
    pub fn certificate_not_after(&self) -> Option<OffsetDateTime> {
        self.certificate_not_after
    }
}

/// All immutable records committed atomically for a successfully acquired crawl.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SuccessfulCrawlRecord {
    pub snapshot: CrawlSnapshotMetadata,
    pub artifact: Option<RawArtifactMetadata>,
    pub dns_observation: CrawlDnsObservation,
    pub tls_observation: CrawlTlsObservation,
    pub detections: Vec<crate::Detection>,
    pub observations: Vec<crate::RuleObservation>,
}

fn required_observation_source(value: &str) -> Result<String, CrawlObservationError> {
    required_observation_value("observation source", value)?
        .ok_or(CrawlObservationError::InvalidSource)
}

fn required_observation_value(
    field: &'static str,
    value: &str,
) -> Result<Option<String>, CrawlObservationError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 512 {
        return Err(CrawlObservationError::InvalidValue { field });
    }
    Ok(Some(value.to_owned()))
}

fn bounded_optional(value: Option<String>) -> Result<Option<String>, CrawlObservationError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| required_observation_value("TLS observation value", &value))
        .transpose()
        .map(Option::flatten)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlObservationError {
    #[error("crawl observation source must be present")]
    InvalidSource,
    #[error("{field} must be present and at most 512 bytes")]
    InvalidValue { field: &'static str },
}

/// Immutable metadata for one raw artifact stored outside PostgreSQL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawArtifactMetadata {
    kind: RawArtifactKind,
    storage_location: String,
    checksum_sha256: String,
    compression: RawArtifactCompression,
    uncompressed_size_bytes: u64,
    compressed_size_bytes: u64,
    retention_expires_at: OffsetDateTime,
}

impl RawArtifactMetadata {
    pub fn response_body(
        storage_location: &str,
        checksum_sha256: &str,
        compression: RawArtifactCompression,
        uncompressed_size_bytes: u64,
        compressed_size_bytes: u64,
        retention_expires_at: OffsetDateTime,
    ) -> Result<Self, RawArtifactMetadataError> {
        let storage_location = storage_location.trim();
        if storage_location.is_empty()
            || storage_location.starts_with('/')
            || storage_location
                .split('/')
                .any(|component| component == "..")
        {
            return Err(RawArtifactMetadataError::InvalidStorageLocation);
        }
        let checksum_sha256 = checksum_sha256.trim();
        if checksum_sha256.len() != 64
            || !checksum_sha256.bytes().all(|byte| {
                byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte.is_ascii_hexdigit())
            })
        {
            return Err(RawArtifactMetadataError::InvalidChecksum);
        }
        if compressed_size_bytes == 0 && uncompressed_size_bytes != 0 {
            return Err(RawArtifactMetadataError::InvalidCompressedSize);
        }

        Ok(Self {
            kind: RawArtifactKind::ResponseBody,
            storage_location: storage_location.to_owned(),
            checksum_sha256: checksum_sha256.to_owned(),
            compression,
            uncompressed_size_bytes,
            compressed_size_bytes,
            retention_expires_at,
        })
    }

    pub fn kind(&self) -> RawArtifactKind {
        self.kind
    }
    pub fn storage_location(&self) -> &str {
        &self.storage_location
    }
    pub fn checksum_sha256(&self) -> &str {
        &self.checksum_sha256
    }
    pub fn compression(&self) -> RawArtifactCompression {
        self.compression
    }
    pub fn uncompressed_size_bytes(&self) -> u64 {
        self.uncompressed_size_bytes
    }
    pub fn compressed_size_bytes(&self) -> u64 {
        self.compressed_size_bytes
    }
    pub fn retention_expires_at(&self) -> OffsetDateTime {
        self.retention_expires_at
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawArtifactKind {
    ResponseBody,
}

impl RawArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResponseBody => "response_body",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawArtifactCompression {
    Zstd,
}

impl RawArtifactCompression {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Zstd => "zstd",
        }
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RawArtifactMetadataError {
    #[error("raw artifact storage location must be a safe relative path")]
    InvalidStorageLocation,
    #[error("raw artifact checksum must be a lowercase SHA-256 digest")]
    InvalidChecksum,
    #[error("non-empty raw artifacts must have compressed content")]
    InvalidCompressedSize,
}

impl CrawlSnapshotMetadata {
    pub fn new(
        requested_url: &str,
        final_url: &str,
        redirects: Vec<CrawlRedirectMetadata>,
        response_status: u16,
        response_headers: BTreeMap<String, String>,
        body_size_bytes: u64,
        captured_at: OffsetDateTime,
    ) -> Result<Self, CrawlSnapshotMetadataError> {
        for name in response_headers.keys() {
            if is_sensitive_header(name) {
                return Err(CrawlSnapshotMetadataError::SensitiveHeader);
            }
        }
        Ok(Self {
            requested_url: required_url("requested URL", requested_url)?,
            final_url: required_url("final URL", final_url)?,
            redirects,
            response_status: validate_status(response_status)?,
            response_headers,
            body_size_bytes,
            captured_at,
            country_observation: None,
        })
    }

    pub fn requested_url(&self) -> &str {
        &self.requested_url
    }
    pub fn final_url(&self) -> &str {
        &self.final_url
    }
    pub fn redirects(&self) -> &[CrawlRedirectMetadata] {
        &self.redirects
    }
    pub fn response_status(&self) -> u16 {
        self.response_status
    }
    pub fn response_headers(&self) -> &BTreeMap<String, String> {
        &self.response_headers
    }
    pub fn body_size_bytes(&self) -> u64 {
        self.body_size_bytes
    }
    pub fn captured_at(&self) -> OffsetDateTime {
        self.captured_at
    }
    pub fn country_observation(&self) -> Option<&CountryObservation> {
        self.country_observation.as_ref()
    }
    pub fn with_country_observation(mut self, observation: Option<CountryObservation>) -> Self {
        self.country_observation = observation;
        self
    }
}

fn required_url(field: &'static str, value: &str) -> Result<String, CrawlSnapshotMetadataError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CrawlSnapshotMetadataError::EmptyUrl { field });
    }
    Ok(value.to_owned())
}

fn validate_status(status: u16) -> Result<u16, CrawlSnapshotMetadataError> {
    if !(100..=599).contains(&status) {
        return Err(CrawlSnapshotMetadataError::InvalidHttpStatus);
    }
    Ok(status)
}

fn is_sensitive_header(name: &str) -> bool {
    let name = name.trim().to_ascii_lowercase();
    matches!(
        name.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "set-cookie2"
            | "x-api-key"
            | "x-auth-token"
            | "x-amz-security-token"
    ) || name.contains("token")
        || name.contains("secret")
        || name == "api-key"
        || name.ends_with("-key")
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlSnapshotMetadataError {
    #[error("{field} must not be empty")]
    EmptyUrl { field: &'static str },
    #[error("HTTP response status must be between 100 and 599")]
    InvalidHttpStatus,
    #[error("snapshot response headers must not contain sensitive values")]
    SensitiveHeader,
}

#[async_trait]
pub trait CrawlWorkerRepository: Send + Sync {
    async fn active_detection_rules(
        &self,
    ) -> Result<Vec<ActiveDetectionRule>, CrawlWorkerRepositoryError>;
    async fn claim_attempt(
        &self,
        job: &CrawlJobV1,
        started_at: OffsetDateTime,
    ) -> Result<ClaimedCrawlAttempt, CrawlWorkerRepositoryError>;

    async fn record_success(
        &self,
        job: &CrawlJobV1,
        record: SuccessfulCrawlRecord,
    ) -> Result<(), CrawlWorkerRepositoryError>;

    async fn record_failure(
        &self,
        job: &CrawlJobV1,
        failure: CrawlFailure,
        finished_at: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), CrawlWorkerRepositoryError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlWorkerRepositoryError {
    #[error("crawl attempt was not found")]
    NotFound,
    #[error("crawl worker persistence is unavailable")]
    Unavailable,
}
