#![forbid(unsafe_code)]

use async_trait::async_trait;
use std::time::Duration;
use thiserror::Error;
use url::{Host, Url};
use uuid::Uuid;

mod admin;
mod crawl_job;
mod csv_import;
mod detection;
mod projection;
mod public;
mod reprocessing;
mod scheduler;
mod search;
mod worker;

pub use admin::{
    AdminActivityKind, AdminAuditCursor, AdminAuditEvent, AdminAuditOperations, AdminCrawlAttempt,
    AdminCrawlPolicy, AdminCrawlRetryState, AdminDetectionRule, AdminDetectionRuleOperations,
    AdminDetectionRuleVersion, AdminImportOperations, AdminOperationError, AdminOperationsActivity,
    AdminOperationsOverview, AdminOperationsRead, AdminPolicyOperations, AdminQueueRead,
    AdminQueueSnapshot, AdminSchedulerOperations, AdminThroughputPoint, AdminWorker,
    WorkerHeartbeat, WorkerHeartbeatOperations,
};
pub use crawl_job::{
    CRAWL_JOB_SCHEMA_VERSION, CorrelationId, CrawlJobError, CrawlJobId, CrawlJobOwner, CrawlJobV1,
    IdempotencyKey,
};
pub use csv_import::{
    CsvDomainImport, CsvImportCommand, CsvImportCommandError, CsvImportParseError,
    CsvImportRepository, CsvImportRepositoryError, CsvImportResult, CsvImportRowError,
    CsvImportRowInput, CsvImportRowResult, CsvImportRowStatus, CsvImportService,
    CsvImportServiceError, parse_row_domain,
};
pub use detection::{
    Detection, DetectionConfidence, DetectionError, DetectionEvidence, DetectionEvidenceSource,
    DetectionMethod, RuleSlug, RuleVersion, TechnologyCategorySlug, TechnologySlug,
};
pub use projection::{
    CurrentTechnology, ProjectionTransition, RuleObservation, RuleObservationStatus,
    TechnologyChange, TechnologyChangeKind, derive_projection,
};
pub use public::{
    PublicAdoption, PublicAdoptionHistory, PublicAdoptionHistoryStatus, PublicAdoptionPoint,
    PublicAdoptionSeries, PublicAnalyticsDiscovery, PublicAnalyticsDomain, PublicAnalyticsMovers,
    PublicAnalyticsOverview, PublicAnalyticsProviderMover, PublicAnalyticsRank,
    PublicAnalyticsRankings, PublicAnalyticsTechnologyMover, PublicChange, PublicChangePage,
    PublicChangeTrend, PublicComparisonCell, PublicComparisonResponse, PublicCountryRank,
    PublicCrawl, PublicCrawlDetail, PublicCrawlPage, PublicDnsObservation, PublicDomainProfile,
    PublicDomainSearchFacets, PublicDomainSearchHit, PublicDomainSearchPage, PublicDomainSummary,
    PublicEvidence, PublicFrequentCrawlDomain, PublicPage, PublicProviderDomainPage,
    PublicProviderProfile, PublicReadError, PublicReadOperations, PublicRedirect,
    PublicRefreshError, PublicRefreshOperations, PublicRefreshRequest, PublicRelatedTechnology,
    PublicTechnology, PublicTechnologyCategory, PublicTechnologyDomainPage,
    PublicTechnologyHistoryPoint, PublicTechnologyLibraryItem, PublicTechnologyLibraryPage,
    PublicTechnologyMigration, PublicTechnologyProfile, PublicTechnologyTrend,
    PublicTlsObservation,
};
pub use reprocessing::{
    AdminReprocessingOperations, AdminReprocessingRun, ReprocessingEvaluation,
    ReprocessingRepository, ReprocessingRepositoryError, ReprocessingWorkItem,
};
pub use scheduler::{
    AdoptionCaptureResult, AdoptionHistoryRebuildResult, AdoptionProjectionOperations,
    CrawlAttemptOutcome, CrawlFailure, CrawlFailureError, CrawlScheduleRepository, PendingCrawlJob,
    SchedulerRepositoryError, SchedulerSettings,
};
pub use search::{SearchDomainProjection, SearchTechnologyProjection};
pub use worker::{
    ActiveDetectionRule, ClaimedCrawlAttempt, CountryObservation, CountryObservationError,
    CrawlDnsObservation, CrawlObservationError, CrawlObservationUnavailableReason,
    CrawlRedirectMetadata, CrawlSnapshotMetadata, CrawlSnapshotMetadataError, CrawlTlsObservation,
    CrawlWorkerRepository, CrawlWorkerRepositoryError, RawArtifactCompression, RawArtifactKind,
    RawArtifactMetadata, RawArtifactMetadataError, SuccessfulCrawlRecord, TlsCertificateValidation,
};

const MEDIUM_CRAWL_INTERVAL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DomainId(Uuid);

impl DomainId {
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CanonicalDomain(String);

impl CanonicalDomain {
    pub fn parse(input: &str) -> Result<Self, CanonicalDomainError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(CanonicalDomainError::Empty);
        }

        if let Some((_, authority_and_rest)) = input.split_once("://")
            && (authority_and_rest.starts_with('/')
                || authority_and_rest.starts_with('?')
                || authority_and_rest.starts_with('#'))
        {
            return Err(CanonicalDomainError::MissingHost);
        }

        let candidate = if input.contains("://") {
            input.to_owned()
        } else {
            format!("https://{input}")
        };
        let url = Url::parse(&candidate).map_err(|_| CanonicalDomainError::InvalidUrl)?;

        if !matches!(url.scheme(), "http" | "https") {
            return Err(CanonicalDomainError::UnsupportedScheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(CanonicalDomainError::CredentialsNotAllowed);
        }

        let Host::Domain(host) = url.host().ok_or(CanonicalDomainError::MissingHost)? else {
            return Err(CanonicalDomainError::IpLiteralNotAllowed);
        };
        let canonical = host.strip_suffix('.').unwrap_or(host);

        if canonical.is_empty() {
            return Err(CanonicalDomainError::InvalidHost);
        }
        if canonical == "localhost" {
            return Err(CanonicalDomainError::LocalhostNotAllowed);
        }
        if canonical.len() > 253 {
            return Err(CanonicalDomainError::TooLong);
        }

        Ok(Self(canonical.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrawlPriority {
    High,
    Medium,
    Low,
}

impl CrawlPriority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }

    pub fn default_interval(self) -> Duration {
        match self {
            Self::High => Duration::from_secs(24 * 60 * 60),
            Self::Medium => MEDIUM_CRAWL_INTERVAL,
            Self::Low => Duration::from_secs(30 * 24 * 60 * 60),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrawlPolicy {
    priority: CrawlPriority,
    desired_interval: Duration,
}

impl CrawlPolicy {
    pub fn new(
        priority: CrawlPriority,
        desired_interval: Duration,
    ) -> Result<Self, CrawlPolicyError> {
        if desired_interval.is_zero() {
            return Err(CrawlPolicyError::ZeroInterval);
        }

        Ok(Self {
            priority,
            desired_interval,
        })
    }

    pub fn priority(&self) -> CrawlPriority {
        self.priority
    }

    pub fn desired_interval(&self) -> Duration {
        self.desired_interval
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomainCreationDefaults {
    crawl_policy: CrawlPolicy,
}

impl DomainCreationDefaults {
    pub fn new(crawl_policy: CrawlPolicy) -> Self {
        Self { crawl_policy }
    }

    pub fn crawl_policy(&self) -> CrawlPolicy {
        self.crawl_policy
    }
}

impl Default for DomainCreationDefaults {
    fn default() -> Self {
        Self {
            crawl_policy: CrawlPolicy {
                priority: CrawlPriority::Medium,
                desired_interval: MEDIUM_CRAWL_INTERVAL,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Domain {
    id: DomainId,
    canonical_domain: CanonicalDomain,
}

impl Domain {
    pub fn new(id: DomainId, canonical_domain: CanonicalDomain) -> Self {
        Self {
            id,
            canonical_domain,
        }
    }

    pub fn id(&self) -> &DomainId {
        &self.id
    }

    pub fn canonical_domain(&self) -> &CanonicalDomain {
        &self.canonical_domain
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDomain {
    canonical_domain: CanonicalDomain,
    crawl_policy: CrawlPolicy,
}

impl NewDomain {
    pub fn new(canonical_domain: CanonicalDomain, crawl_policy: CrawlPolicy) -> Self {
        Self {
            canonical_domain,
            crawl_policy,
        }
    }

    pub fn canonical_domain(&self) -> &CanonicalDomain {
        &self.canonical_domain
    }

    pub fn crawl_policy(&self) -> CrawlPolicy {
        self.crawl_policy
    }
}

#[async_trait]
pub trait DomainRepository: Send + Sync {
    async fn create(&self, domain: NewDomain) -> Result<Domain, DomainRepositoryError>;

    async fn create_with_audit(
        &self,
        domain: NewDomain,
        actor_subject: &str,
    ) -> Result<Domain, DomainRepositoryError>;

    async fn find_active_by_canonical(
        &self,
        canonical_domain: &CanonicalDomain,
    ) -> Result<Option<Domain>, DomainRepositoryError>;

    async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<Vec<Domain>, DomainRepositoryError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainListPage {
    domains: Vec<Domain>,
    next_cursor: Option<CanonicalDomain>,
}

impl DomainListPage {
    pub fn domains(&self) -> &[Domain] {
        &self.domains
    }

    pub fn next_cursor(&self) -> Option<&CanonicalDomain> {
        self.next_cursor.as_ref()
    }
}

#[async_trait]
pub trait AdminDomainOperations: Send + Sync {
    async fn create_manual(
        &self,
        input: &str,
        actor_subject: &str,
    ) -> Result<Domain, DomainServiceError>;

    async fn find_active(&self, input: &str) -> Result<Option<Domain>, DomainServiceError>;

    async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<DomainListPage, DomainServiceError>;
}

pub struct DomainService<R> {
    repository: R,
    defaults: DomainCreationDefaults,
}

impl<R> DomainService<R>
where
    R: DomainRepository,
{
    pub fn new(repository: R, defaults: DomainCreationDefaults) -> Self {
        Self {
            repository,
            defaults,
        }
    }

    pub fn with_default_policy(repository: R) -> Self {
        Self::new(repository, DomainCreationDefaults::default())
    }

    pub async fn create(&self, input: &str) -> Result<Domain, DomainServiceError> {
        let canonical_domain = CanonicalDomain::parse(input)?;
        let domain = NewDomain::new(canonical_domain, self.defaults.crawl_policy());

        self.repository.create(domain).await.map_err(Into::into)
    }

    pub async fn find_active(&self, input: &str) -> Result<Option<Domain>, DomainServiceError> {
        let canonical_domain = CanonicalDomain::parse(input)?;

        self.repository
            .find_active_by_canonical(&canonical_domain)
            .await
            .map_err(Into::into)
    }

    pub async fn create_manual(
        &self,
        input: &str,
        actor_subject: &str,
    ) -> Result<Domain, DomainServiceError> {
        let canonical_domain = CanonicalDomain::parse(input)?;
        let domain = NewDomain::new(canonical_domain, self.defaults.crawl_policy());

        self.repository
            .create_with_audit(domain, actor_subject)
            .await
            .map_err(Into::into)
    }

    pub async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<DomainListPage, DomainServiceError> {
        let fetch_limit = limit
            .checked_add(1)
            .ok_or(DomainServiceError::Unavailable)?;
        let mut domains = self
            .repository
            .list_active(after, fetch_limit)
            .await
            .map_err(DomainServiceError::from)?;
        let next_cursor = if domains.len() > limit {
            domains
                .get(limit.saturating_sub(1))
                .map(|domain| domain.canonical_domain().clone())
        } else {
            None
        };
        domains.truncate(limit);

        Ok(DomainListPage {
            domains,
            next_cursor,
        })
    }
}

#[async_trait]
impl<R> AdminDomainOperations for DomainService<R>
where
    R: DomainRepository,
{
    async fn create_manual(
        &self,
        input: &str,
        actor_subject: &str,
    ) -> Result<Domain, DomainServiceError> {
        self.create_manual(input, actor_subject).await
    }

    async fn find_active(&self, input: &str) -> Result<Option<Domain>, DomainServiceError> {
        self.find_active(input).await
    }

    async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<DomainListPage, DomainServiceError> {
        self.list_active(after, limit).await
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CanonicalDomainError {
    #[error("domain input is empty")]
    Empty,
    #[error("domain input is not a valid URL or domain")]
    InvalidUrl,
    #[error("domain URL must use HTTP or HTTPS")]
    UnsupportedScheme,
    #[error("domain URL must not include credentials")]
    CredentialsNotAllowed,
    #[error("domain URL does not include a host")]
    MissingHost,
    #[error("domain host is invalid")]
    InvalidHost,
    #[error("IP literals are not valid domain targets")]
    IpLiteralNotAllowed,
    #[error("localhost is not a valid domain target")]
    LocalhostNotAllowed,
    #[error("domain exceeds the DNS hostname length limit")]
    TooLong,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlPolicyError {
    #[error("crawl policy interval must be greater than zero")]
    ZeroInterval,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DomainRepositoryError {
    #[error("an active domain with this canonical identity already exists")]
    Duplicate,
    #[error("domain repository is unavailable")]
    Unavailable,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum DomainServiceError {
    #[error(transparent)]
    InvalidInput(#[from] CanonicalDomainError),
    #[error("an active domain with this canonical identity already exists")]
    Duplicate,
    #[error("domain service is unavailable")]
    Unavailable,
}

impl From<DomainRepositoryError> for DomainServiceError {
    fn from(error: DomainRepositoryError) -> Self {
        match error {
            DomainRepositoryError::Duplicate => Self::Duplicate,
            DomainRepositoryError::Unavailable => Self::Unavailable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn canonical_domain_normalizes_supported_input_forms() {
        let examples = [
            (" Example.COM ", "example.com"),
            (
                "https://Example.COM/path?query=value#fragment",
                "example.com",
            ),
            ("example.com:8443/path", "example.com"),
            (
                "https://www.b\u{fc}cher.example./catalog",
                "www.xn--bcher-kva.example",
            ),
        ];

        for (input, expected) in examples {
            assert_eq!(CanonicalDomain::parse(input).unwrap().as_str(), expected);
        }
    }

    #[test]
    fn canonical_domain_rejects_unsafe_or_invalid_target_forms() {
        let examples = [
            ("", CanonicalDomainError::Empty),
            ("ftp://example.com", CanonicalDomainError::UnsupportedScheme),
            (
                "https://user:password@example.com",
                CanonicalDomainError::CredentialsNotAllowed,
            ),
            ("https:///path", CanonicalDomainError::MissingHost),
            ("127.0.0.1", CanonicalDomainError::IpLiteralNotAllowed),
            ("[::1]", CanonicalDomainError::IpLiteralNotAllowed),
            ("localhost", CanonicalDomainError::LocalhostNotAllowed),
        ];

        for (input, expected) in examples {
            assert_eq!(CanonicalDomain::parse(input), Err(expected));
        }
    }

    #[test]
    fn crawl_policy_rejects_a_zero_interval() {
        assert_eq!(
            CrawlPolicy::new(CrawlPriority::Medium, Duration::ZERO),
            Err(CrawlPolicyError::ZeroInterval)
        );
    }

    struct CapturingRepository {
        received: Mutex<Option<NewDomain>>,
    }

    #[async_trait]
    impl DomainRepository for Arc<CapturingRepository> {
        async fn create(&self, domain: NewDomain) -> Result<Domain, DomainRepositoryError> {
            *self.received.lock().unwrap() = Some(domain.clone());

            Ok(Domain::new(
                DomainId::from_uuid(Uuid::nil()),
                domain.canonical_domain().clone(),
            ))
        }

        async fn find_active_by_canonical(
            &self,
            _: &CanonicalDomain,
        ) -> Result<Option<Domain>, DomainRepositoryError> {
            Ok(None)
        }

        async fn create_with_audit(
            &self,
            domain: NewDomain,
            _: &str,
        ) -> Result<Domain, DomainRepositoryError> {
            self.create(domain).await
        }

        async fn list_active(
            &self,
            _: Option<&CanonicalDomain>,
            _: usize,
        ) -> Result<Vec<Domain>, DomainRepositoryError> {
            Ok(Vec::new())
        }
    }

    struct DuplicateRepository;

    #[async_trait]
    impl DomainRepository for DuplicateRepository {
        async fn create(&self, _: NewDomain) -> Result<Domain, DomainRepositoryError> {
            Err(DomainRepositoryError::Duplicate)
        }

        async fn find_active_by_canonical(
            &self,
            _: &CanonicalDomain,
        ) -> Result<Option<Domain>, DomainRepositoryError> {
            Ok(None)
        }

        async fn create_with_audit(
            &self,
            _: NewDomain,
            _: &str,
        ) -> Result<Domain, DomainRepositoryError> {
            Err(DomainRepositoryError::Duplicate)
        }

        async fn list_active(
            &self,
            _: Option<&CanonicalDomain>,
            _: usize,
        ) -> Result<Vec<Domain>, DomainRepositoryError> {
            Ok(Vec::new())
        }
    }

    #[tokio::test]
    async fn service_persists_normalized_input_with_the_default_policy() {
        let repository = Arc::new(CapturingRepository {
            received: Mutex::new(None),
        });
        let service = DomainService::with_default_policy(Arc::clone(&repository));

        let domain = service.create("HTTPS://Example.COM/path").await.unwrap();

        assert_eq!(domain.canonical_domain().as_str(), "example.com");
        let received = repository.received.lock().unwrap().clone().unwrap();
        assert_eq!(received.canonical_domain().as_str(), "example.com");
        assert_eq!(received.crawl_policy().priority(), CrawlPriority::Medium);
        assert_eq!(
            received.crawl_policy().desired_interval(),
            MEDIUM_CRAWL_INTERVAL
        );
    }

    #[tokio::test]
    async fn service_preserves_duplicate_outcomes() {
        let service = DomainService::with_default_policy(DuplicateRepository);

        assert_eq!(
            service.create("Example.COM").await,
            Err(DomainServiceError::Duplicate)
        );
    }
}
