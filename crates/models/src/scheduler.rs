use crate::{CrawlJobId, CrawlJobV1};
use async_trait::async_trait;
use std::time::Duration;
use thiserror::Error;
use time::{Date, OffsetDateTime};

const DEFAULT_MAX_CRAWL_RETRIES: u8 = 3;
const DEFAULT_CRAWL_RETRY_BASE: Duration = Duration::from_secs(5 * 60);
const DEFAULT_MAX_OUTBOX_PUBLISH_ATTEMPTS: u8 = 5;
const DEFAULT_OUTBOX_RETRY_BASE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedulerSettings {
    max_crawl_retries: u8,
    crawl_retry_base: Duration,
    max_outbox_publish_attempts: u8,
    outbox_retry_base: Duration,
}

impl Default for SchedulerSettings {
    fn default() -> Self {
        Self {
            max_crawl_retries: DEFAULT_MAX_CRAWL_RETRIES,
            crawl_retry_base: DEFAULT_CRAWL_RETRY_BASE,
            max_outbox_publish_attempts: DEFAULT_MAX_OUTBOX_PUBLISH_ATTEMPTS,
            outbox_retry_base: DEFAULT_OUTBOX_RETRY_BASE,
        }
    }
}

impl SchedulerSettings {
    pub fn new(
        max_crawl_retries: u8,
        crawl_retry_base: Duration,
        max_outbox_publish_attempts: u8,
        outbox_retry_base: Duration,
    ) -> Result<Self, SchedulerSettingsError> {
        if crawl_retry_base.is_zero() {
            return Err(SchedulerSettingsError::CrawlRetryBase);
        }
        if max_outbox_publish_attempts == 0 {
            return Err(SchedulerSettingsError::OutboxPublishLimit);
        }
        if outbox_retry_base.is_zero() {
            return Err(SchedulerSettingsError::OutboxRetryBase);
        }

        Ok(Self {
            max_crawl_retries,
            crawl_retry_base,
            max_outbox_publish_attempts,
            outbox_retry_base,
        })
    }

    pub fn max_crawl_retries(self) -> u8 {
        self.max_crawl_retries
    }

    pub fn max_outbox_publish_attempts(self) -> u8 {
        self.max_outbox_publish_attempts
    }

    pub fn crawl_retry_delay(self, retry_number: u8) -> Option<Duration> {
        exponential_delay(self.crawl_retry_base, retry_number, self.max_crawl_retries)
    }

    pub fn outbox_retry_delay(self, attempt_number: u8) -> Option<Duration> {
        exponential_delay(
            self.outbox_retry_base,
            attempt_number,
            self.max_outbox_publish_attempts,
        )
    }
}

fn exponential_delay(base: Duration, number: u8, maximum: u8) -> Option<Duration> {
    if number == 0 || number > maximum {
        return None;
    }

    base.checked_mul(1_u32.checked_shl(u32::from(number - 1))?)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SchedulerSettingsError {
    #[error("crawl retry base delay must be greater than zero")]
    CrawlRetryBase,
    #[error("outbox publish attempt limit must be greater than zero")]
    OutboxPublishLimit,
    #[error("outbox retry base delay must be greater than zero")]
    OutboxRetryBase,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingCrawlJob {
    job: CrawlJobV1,
    available_at: OffsetDateTime,
    publish_attempt_count: u8,
}

impl PendingCrawlJob {
    pub fn new(job: CrawlJobV1, available_at: OffsetDateTime, publish_attempt_count: u8) -> Self {
        Self {
            job,
            available_at,
            publish_attempt_count,
        }
    }

    pub fn job(&self) -> &CrawlJobV1 {
        &self.job
    }

    pub fn available_at(&self) -> OffsetDateTime {
        self.available_at
    }

    pub fn publish_attempt_count(&self) -> u8 {
        self.publish_attempt_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrawlFailure {
    code: String,
    summary: String,
}

impl CrawlFailure {
    pub fn new(code: &str, summary: &str) -> Result<Self, CrawlFailureError> {
        let code = code.trim();
        let summary = summary.trim();
        if code.is_empty() {
            return Err(CrawlFailureError::EmptyCode);
        }
        if summary.is_empty() {
            return Err(CrawlFailureError::EmptySummary);
        }
        if summary.len() > 1_024 {
            return Err(CrawlFailureError::SummaryTooLong);
        }

        Ok(Self {
            code: code.to_owned(),
            summary: summary.to_owned(),
        })
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum CrawlFailureError {
    #[error("crawl failure code must not be empty")]
    EmptyCode,
    #[error("crawl failure summary must not be empty")]
    EmptySummary,
    #[error("crawl failure summary must not exceed 1024 bytes")]
    SummaryTooLong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CrawlAttemptOutcome {
    Succeeded,
    Failed(CrawlFailure),
}

#[async_trait]
pub trait CrawlScheduleRepository: Send + Sync {
    async fn reserve_due(
        &self,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<CrawlJobV1>, SchedulerRepositoryError>;

    async fn pending_publications(
        &self,
        now: OffsetDateTime,
        limit: usize,
    ) -> Result<Vec<PendingCrawlJob>, SchedulerRepositoryError>;

    async fn mark_published(
        &self,
        job_id: &CrawlJobId,
        now: OffsetDateTime,
    ) -> Result<(), SchedulerRepositoryError>;

    async fn record_publish_failure(
        &self,
        job_id: &CrawlJobId,
        now: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), SchedulerRepositoryError>;

    async fn record_attempt_outcome(
        &self,
        job_id: &CrawlJobId,
        outcome: CrawlAttemptOutcome,
        now: OffsetDateTime,
        settings: SchedulerSettings,
    ) -> Result<(), SchedulerRepositoryError>;
}

/// Rebuildable, daily public adoption projection captured by the scheduler.
/// It is deliberately separate from crawl job reservation and publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdoptionCaptureResult {
    pub inserted_rows: u64,
    pub already_captured: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdoptionHistoryRebuildResult {
    pub rebuilt_days: u64,
    pub inserted_rows: u64,
}

#[async_trait]
pub trait AdoptionProjectionOperations: Send + Sync {
    async fn capture_daily_adoption(
        &self,
        observed_on: Date,
        captured_at: OffsetDateTime,
    ) -> Result<AdoptionCaptureResult, SchedulerRepositoryError>;

    async fn rebuild_adoption_history(
        &self,
        from: Date,
        to: Date,
        rebuilt_at: OffsetDateTime,
    ) -> Result<AdoptionHistoryRebuildResult, SchedulerRepositoryError>;
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SchedulerRepositoryError {
    #[error("scheduler record was not found")]
    NotFound,
    #[error("scheduler persistence is unavailable")]
    Unavailable,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CrawlPriority;

    #[test]
    fn uses_the_selected_crawl_retry_backoff() {
        let settings = SchedulerSettings::default();

        assert_eq!(
            settings.crawl_retry_delay(1),
            Some(Duration::from_secs(5 * 60))
        );
        assert_eq!(
            settings.crawl_retry_delay(2),
            Some(Duration::from_secs(10 * 60))
        );
        assert_eq!(
            settings.crawl_retry_delay(3),
            Some(Duration::from_secs(20 * 60))
        );
        assert_eq!(settings.crawl_retry_delay(4), None);
    }

    #[test]
    fn exposes_priority_cadence_defaults() {
        assert_eq!(
            CrawlPriority::High.default_interval(),
            Duration::from_secs(24 * 60 * 60)
        );
        assert_eq!(
            CrawlPriority::Medium.default_interval(),
            Duration::from_secs(7 * 24 * 60 * 60)
        );
        assert_eq!(
            CrawlPriority::Low.default_interval(),
            Duration::from_secs(30 * 24 * 60 * 60)
        );
    }
}
