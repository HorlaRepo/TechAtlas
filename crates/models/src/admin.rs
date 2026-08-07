use crate::{CanonicalDomain, CrawlPriority, CsvDomainImport, CsvImportResult};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminOperationsOverview {
    pub domain_count: u64,
    pub technology_count: u64,
    pub current_detection_count: u64,
    pub successful_crawl_count: u64,
    pub scheduled_crawl_count: u64,
    pub throughput: Vec<AdminThroughputPoint>,
    pub workers: Vec<AdminWorker>,
    pub activity: Vec<AdminOperationsActivity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminThroughputPoint {
    pub observed_at: OffsetDateTime,
    pub completed_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminWorker {
    pub name: String,
    pub region: String,
    pub in_flight_work: u32,
    pub completed_total: u64,
    pub last_heartbeat_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminOperationsActivity {
    pub id: String,
    pub title: String,
    pub description: String,
    pub occurred_at: OffsetDateTime,
    pub kind: AdminActivityKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminActivityKind {
    Discovery,
    Success,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdminQueueSnapshot {
    pub ready_count: u64,
    pub processing_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkerHeartbeat {
    pub name: String,
    pub region: String,
    pub in_flight_work: u32,
    pub observed_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AdminCrawlPolicy {
    pub is_enabled: bool,
    pub priority: String,
    pub desired_interval_hours: u16,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AdminAuditEvent {
    pub id: String,
    pub actor_subject: String,
    pub action: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub occurred_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminAuditCursor {
    occurred_at: OffsetDateTime,
    id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminCrawlAttempt {
    pub job_id: String,
    pub canonical_domain: String,
    pub attempt_number: u16,
    pub status: String,
    pub queued_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub failure_code: Option<String>,
    pub failure_summary: Option<String>,
    pub retry_state: AdminCrawlRetryState,
    pub retry_eligible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminImportBatch {
    pub import_id: String,
    pub source_name: String,
    pub completed_at: OffsetDateTime,
    pub domain_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminImportBatchRecrawl {
    pub import_id: String,
    pub source_name: String,
    pub requested_domain_count: u64,
    pub scheduled_domain_count: u64,
    pub skipped_domain_count: u64,
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdminCrawlRetryState {
    InProgress,
    AutomaticRetryScheduled,
    ManualRetryAvailable,
    ManualRetryBlocked,
    NotApplicable,
}

impl AdminCrawlRetryState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::AutomaticRetryScheduled => "automatic_retry_scheduled",
            Self::ManualRetryAvailable => "manual_retry_available",
            Self::ManualRetryBlocked => "manual_retry_blocked",
            Self::NotApplicable => "not_applicable",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminDetectionRule {
    pub rule_slug: String,
    pub technology_slug: String,
    pub technology_name: String,
    pub active_version: Option<u16>,
    pub draft_definition: Option<Value>,
    pub draft_updated_at: Option<OffsetDateTime>,
    pub versions: Vec<AdminDetectionRuleVersion>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdminDetectionRuleVersion {
    pub version: u16,
    pub definition: Value,
    pub published_at: OffsetDateTime,
    pub is_active: bool,
}

impl AdminAuditCursor {
    pub fn new(occurred_at: OffsetDateTime, id: String) -> Result<Self, AdminOperationError> {
        if id.trim().is_empty() {
            return Err(AdminOperationError::Validation);
        }
        Ok(Self { occurred_at, id })
    }

    pub fn occurred_at(&self) -> OffsetDateTime {
        self.occurred_at
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

#[async_trait]
pub trait AdminPolicyOperations: Send + Sync {
    async fn update_policy(
        &self,
        domain: &CanonicalDomain,
        policy: AdminCrawlPolicy,
        actor_subject: &str,
    ) -> Result<AdminCrawlPolicy, AdminOperationError>;
}

#[async_trait]
pub trait AdminImportOperations: Send + Sync {
    async fn import_csv(
        &self,
        source_name: &str,
        actor_subject: &str,
        document: CsvDomainImport,
    ) -> Result<CsvImportResult, AdminOperationError>;

    async fn completed_import_batches(
        &self,
        limit: usize,
    ) -> Result<Vec<AdminImportBatch>, AdminOperationError>;
}

#[async_trait]
pub trait AdminAuditOperations: Send + Sync {
    async fn audit_events(
        &self,
        cursor: Option<AdminAuditCursor>,
        limit: usize,
    ) -> Result<(Vec<AdminAuditEvent>, Option<AdminAuditCursor>), AdminOperationError>;
}

#[async_trait]
pub trait AdminOperationsRead: Send + Sync {
    async fn overview(
        &self,
        now: OffsetDateTime,
    ) -> Result<AdminOperationsOverview, AdminOperationError>;
}

#[async_trait]
pub trait AdminQueueRead: Send + Sync {
    async fn snapshot(&self) -> Result<AdminQueueSnapshot, AdminOperationError>;
}

#[async_trait]
pub trait AdminSchedulerOperations: Send + Sync {
    /// Makes an existing enabled domain eligible for the scheduler without directly enqueuing work.
    async fn request_domain_crawl(
        &self,
        domain: &CanonicalDomain,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<(), AdminOperationError>;
    async fn crawl_attempts(
        &self,
        limit: usize,
    ) -> Result<Vec<AdminCrawlAttempt>, AdminOperationError>;
    async fn retry_terminal_attempt(
        &self,
        job_id: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminCrawlAttempt, AdminOperationError>;
    async fn schedule_country_enrichment_recrawl(
        &self,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<u64, AdminOperationError>;
    /// Makes the active enabled domains in one completed import eligible for scheduler processing.
    async fn schedule_import_batch_recrawl(
        &self,
        import_id: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminImportBatchRecrawl, AdminOperationError>;
}

#[async_trait]
pub trait AdminDetectionRuleOperations: Send + Sync {
    async fn rules(&self) -> Result<Vec<AdminDetectionRule>, AdminOperationError>;
    async fn save_draft(
        &self,
        rule_slug: &str,
        definition: Value,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminDetectionRule, AdminOperationError>;
    async fn publish_draft(
        &self,
        rule_slug: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminDetectionRuleVersion, AdminOperationError>;
    async fn mark_draft_tested(
        &self,
        rule_slug: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<(), AdminOperationError>;
    async fn set_active_version(
        &self,
        rule_slug: &str,
        version: Option<u16>,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<(), AdminOperationError>;
}

#[async_trait]
pub trait WorkerHeartbeatOperations: Send + Sync {
    async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), AdminOperationError>;
    async fn record_completion(
        &self,
        heartbeat: WorkerHeartbeat,
    ) -> Result<(), AdminOperationError>;
}

impl AdminCrawlPolicy {
    pub fn new(
        is_enabled: bool,
        priority: CrawlPriority,
        desired_interval_hours: u16,
    ) -> Result<Self, AdminOperationError> {
        if !(1..=8_760).contains(&desired_interval_hours) {
            return Err(AdminOperationError::Validation);
        }
        Ok(Self {
            is_enabled,
            priority: priority.as_str().to_owned(),
            desired_interval_hours,
        })
    }
    pub fn priority_enum(&self) -> Result<CrawlPriority, AdminOperationError> {
        match self.priority.as_str() {
            "high" => Ok(CrawlPriority::High),
            "medium" => Ok(CrawlPriority::Medium),
            "low" => Ok(CrawlPriority::Low),
            _ => Err(AdminOperationError::Validation),
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum AdminOperationError {
    #[error("admin resource was not found")]
    NotFound,
    #[error("admin input is invalid")]
    Validation,
    #[error("admin operation is not eligible in the current state")]
    Conflict,
    #[error("admin persistence is unavailable")]
    Unavailable,
}
