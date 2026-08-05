use crate::{ActiveDetectionRule, Detection, RawArtifactMetadata, RuleObservation};
use async_trait::async_trait;
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdminReprocessingRun {
    pub id: String,
    pub rule_slug: String,
    pub rule_version: u16,
    pub status: String,
    pub total_snapshot_count: u32,
    pub succeeded_snapshot_count: u32,
    pub failed_snapshot_count: u32,
    pub requested_at: OffsetDateTime,
    pub started_at: Option<OffsetDateTime>,
    pub finished_at: Option<OffsetDateTime>,
}

#[async_trait]
pub trait AdminReprocessingOperations: Send + Sync {
    async fn start_reprocessing(
        &self,
        rule_slug: &str,
        rule_version: u16,
        idempotency_key: &str,
        actor_subject: &str,
        now: OffsetDateTime,
    ) -> Result<AdminReprocessingRun, AdminOperationError>;

    async fn reprocessing_runs(
        &self,
        rule_slug: Option<&str>,
        limit: usize,
    ) -> Result<Vec<AdminReprocessingRun>, AdminOperationError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReprocessingWorkItem {
    pub id: Uuid,
    pub run_id: Uuid,
    pub snapshot_id: Uuid,
    pub domain_id: Uuid,
    pub technology_id: Uuid,
    pub detection_rule_version_id: Uuid,
    pub captured_at: OffsetDateTime,
    pub attempt_count: u16,
    pub response_headers: BTreeMap<String, String>,
    pub artifact: Option<RawArtifactMetadata>,
    pub rule: ActiveDetectionRule,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReprocessingEvaluation {
    pub detection: Option<Detection>,
    pub observation: RuleObservation,
    pub technology_version: Option<String>,
}

#[async_trait]
pub trait ReprocessingRepository: Send + Sync {
    async fn claim_next_reprocessing_item(
        &self,
        now: OffsetDateTime,
        max_attempts: u16,
    ) -> Result<Option<ReprocessingWorkItem>, ReprocessingRepositoryError>;

    async fn record_reprocessing_success(
        &self,
        item: &ReprocessingWorkItem,
        evaluation: ReprocessingEvaluation,
        completed_at: OffsetDateTime,
    ) -> Result<(), ReprocessingRepositoryError>;

    async fn record_reprocessing_failure(
        &self,
        item_id: Uuid,
        summary: &str,
        retry_at: OffsetDateTime,
        terminal: bool,
        completed_at: OffsetDateTime,
    ) -> Result<(), ReprocessingRepositoryError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReprocessingRepositoryError {
    #[error("reprocessing work is unavailable")]
    Unavailable,
}

use crate::AdminOperationError;
