use crate::{
    admin_auth::AdminAuthorizer,
    admin_domains, admin_operations,
    health::{healthz, readyz},
    metrics, public,
};
use async_trait::async_trait;
use axum::{Router, middleware, routing::get};
use std::sync::Arc;
use techatlas_common::DependencyProbe;
use techatlas_models::{
    AdminAuditOperations, AdminDetectionRuleOperations, AdminDomainOperations,
    AdminImportOperations, AdminOperationError, AdminOperationsRead, AdminPolicyOperations,
    AdminQueueRead, AdminReprocessingOperations, AdminSchedulerOperations, CanonicalDomain,
    PublicReadError, PublicReadOperations, PublicRefreshError, PublicRefreshOperations,
    PublicRefreshRequest,
};
use techatlas_search::{DomainSearch, DomainSearchPage, DomainSearchQuery, SearchIndexError};
use techatlas_telemetry::TelemetryMetrics;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub metrics: Arc<TelemetryMetrics>,
    pub postgres: Arc<dyn DependencyProbe>,
    pub redis: Arc<dyn DependencyProbe>,
    pub meilisearch: Arc<dyn DependencyProbe>,
    pub admin_domains: Arc<dyn AdminDomainOperations>,
    pub admin_auth: Arc<dyn AdminAuthorizer>,
    pub admin_imports: Arc<dyn AdminImportOperations>,
    pub admin_policies: Arc<dyn AdminPolicyOperations>,
    pub admin_audits: Arc<dyn AdminAuditOperations>,
    pub admin_operations: Arc<dyn AdminOperationsRead>,
    pub admin_queue: Arc<dyn AdminQueueRead>,
    pub admin_scheduler: Arc<dyn AdminSchedulerOperations>,
    pub admin_rules: Arc<dyn AdminDetectionRuleOperations>,
    pub admin_reprocessing: Arc<dyn AdminReprocessingOperations>,
    pub public_reads: Arc<dyn PublicReadOperations>,
    pub public_refreshes: Arc<dyn PublicRefreshOperations>,
    pub public_search: Arc<dyn DomainSearch>,
    pub public_stale_after_days: u16,
    pub public_refresh_cooldown_hours: u16,
}

/// Test-only composition roots can use these explicit unavailable adapters.
pub struct UnavailablePublicReads;
pub struct UnavailablePublicRefreshes;
pub struct UnavailableAdminOperations;
#[async_trait]
impl AdminImportOperations for UnavailableAdminOperations {
    async fn import_csv(
        &self,
        _: &str,
        _: &str,
        _: techatlas_models::CsvDomainImport,
    ) -> Result<techatlas_models::CsvImportResult, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminPolicyOperations for UnavailableAdminOperations {
    async fn update_policy(
        &self,
        _: &CanonicalDomain,
        _: techatlas_models::AdminCrawlPolicy,
        _: &str,
    ) -> Result<techatlas_models::AdminCrawlPolicy, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminAuditOperations for UnavailableAdminOperations {
    async fn audit_events(
        &self,
        _: Option<techatlas_models::AdminAuditCursor>,
        _: usize,
    ) -> Result<
        (
            Vec<techatlas_models::AdminAuditEvent>,
            Option<techatlas_models::AdminAuditCursor>,
        ),
        AdminOperationError,
    > {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminOperationsRead for UnavailableAdminOperations {
    async fn overview(
        &self,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::AdminOperationsOverview, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminReprocessingOperations for UnavailableAdminOperations {
    async fn start_reprocessing(
        &self,
        _: &str,
        _: u16,
        _: &str,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::AdminReprocessingRun, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn reprocessing_runs(
        &self,
        _: Option<&str>,
        _: usize,
    ) -> Result<Vec<techatlas_models::AdminReprocessingRun>, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminQueueRead for UnavailableAdminOperations {
    async fn snapshot(&self) -> Result<techatlas_models::AdminQueueSnapshot, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminSchedulerOperations for UnavailableAdminOperations {
    async fn request_domain_crawl(
        &self,
        _: &CanonicalDomain,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn crawl_attempts(
        &self,
        _: usize,
    ) -> Result<Vec<techatlas_models::AdminCrawlAttempt>, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn retry_terminal_attempt(
        &self,
        _: &str,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::AdminCrawlAttempt, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn schedule_country_enrichment_recrawl(
        &self,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<u64, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl AdminDetectionRuleOperations for UnavailableAdminOperations {
    async fn rules(
        &self,
    ) -> Result<Vec<techatlas_models::AdminDetectionRule>, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn save_draft(
        &self,
        _: &str,
        _: serde_json::Value,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::AdminDetectionRule, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn publish_draft(
        &self,
        _: &str,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::AdminDetectionRuleVersion, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn mark_draft_tested(
        &self,
        _: &str,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
    async fn set_active_version(
        &self,
        _: &str,
        _: Option<u16>,
        _: &str,
        _: time::OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }
}
#[async_trait]
impl PublicReadOperations for UnavailablePublicReads {
    async fn domain_profile(
        &self,
        _: &CanonicalDomain,
    ) -> Result<Option<techatlas_models::PublicDomainProfile>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn domain_changes(
        &self,
        _: &CanonicalDomain,
        _: Option<time::OffsetDateTime>,
        _: usize,
    ) -> Result<techatlas_models::PublicPage<techatlas_models::PublicChange>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn domain_crawls(
        &self,
        _: &CanonicalDomain,
        _: Option<time::OffsetDateTime>,
        _: usize,
    ) -> Result<techatlas_models::PublicPage<techatlas_models::PublicCrawl>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn crawl_detail(
        &self,
        _: &CanonicalDomain,
        _: &str,
    ) -> Result<Option<techatlas_models::PublicCrawlDetail>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn technologies(
        &self,
        _: Option<&str>,
        _: Option<techatlas_models::PublicTechnologyTrend>,
        _: Option<&str>,
        _: usize,
        _: time::OffsetDateTime,
    ) -> Result<techatlas_models::PublicTechnologyLibraryPage, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn technology_domains(
        &self,
        _: &str,
        _: Option<&str>,
        _: usize,
    ) -> Result<
        Option<techatlas_models::PublicPage<techatlas_models::PublicDomainSummary>>,
        PublicReadError,
    > {
        Err(PublicReadError::Unavailable)
    }
    async fn technology_profile(
        &self,
        _: &str,
        _: Option<&str>,
        _: usize,
        _: time::OffsetDateTime,
    ) -> Result<Option<techatlas_models::PublicTechnologyProfile>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn comparison(
        &self,
        _: &[CanonicalDomain],
        _: time::OffsetDateTime,
    ) -> Result<Vec<techatlas_models::PublicComparisonCell>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn analytics_overview(
        &self,
    ) -> Result<techatlas_models::PublicAnalyticsOverview, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn adoption(&self) -> Result<Vec<techatlas_models::PublicAdoption>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn adoption_history(
        &self,
        _: time::Date,
        _: Option<&str>,
    ) -> Result<techatlas_models::PublicAdoptionHistory, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn analytics_rankings(
        &self,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsRankings, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn analytics_movers(
        &self,
        _: time::OffsetDateTime,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsMovers, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn change_trends(
        &self,
        _: time::OffsetDateTime,
    ) -> Result<Vec<techatlas_models::PublicChangeTrend>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn analytics_discovery(
        &self,
        _: time::OffsetDateTime,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsDiscovery, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
    async fn provider_profile(
        &self,
        _: &str,
        _: Option<&str>,
        _: usize,
        _: time::OffsetDateTime,
    ) -> Result<Option<techatlas_models::PublicProviderProfile>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }
}

#[async_trait]
impl PublicRefreshOperations for UnavailablePublicRefreshes {
    async fn request_refresh(
        &self,
        _: &CanonicalDomain,
        _: time::OffsetDateTime,
        _: time::Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError> {
        Err(PublicRefreshError::Unavailable)
    }
}

pub struct UnavailablePublicSearch;
#[async_trait]
impl DomainSearch for UnavailablePublicSearch {
    async fn search(&self, _: DomainSearchQuery) -> Result<DomainSearchPage, SearchIndexError> {
        Err(SearchIndexError::Unavailable)
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics::metrics))
        .merge(admin_domains::router())
        .merge(admin_operations::router())
        .merge(public::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            metrics::observe_http,
        ))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}
