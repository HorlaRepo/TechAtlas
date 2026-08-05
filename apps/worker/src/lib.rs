#![forbid(unsafe_code)]

use async_trait::async_trait;
use std::{net::IpAddr, sync::Arc, time::Duration};
use techatlas_crawler::{
    Capture, CaptureUnavailableReason, CrawlError, CrawlRedirect, CrawlResponse, CrawlUrl, Crawler,
    DnsCapture, PolitenessGate, PolitenessGateError, PolitenessRequest, TlsCapture, TlsValidation,
    sanitized_html_for_storage,
};
use techatlas_detector::{DetectorError, evaluate_active, evaluate_reprocessing};
use techatlas_models::{
    ClaimedCrawlAttempt, CountryObservation, CrawlDnsObservation, CrawlFailure, CrawlFailureError,
    CrawlObservationError, CrawlObservationUnavailableReason, CrawlRedirectMetadata,
    CrawlSnapshotMetadata, CrawlSnapshotMetadataError, CrawlTlsObservation, CrawlWorkerRepository,
    CrawlWorkerRepositoryError, ReprocessingRepository, ReprocessingRepositoryError,
    SchedulerSettings, SuccessfulCrawlRecord, TlsCertificateValidation,
};
use techatlas_parser::{ArtifactBundle, ArtifactInput, UnavailableReason, parse};
use techatlas_queue::{
    CrawlJobConsumer, CrawlJobConsumerName, CrawlJobQueueError, ReceivedCrawlJob,
};
use techatlas_storage::RawArtifactStore;
use techatlas_telemetry::TelemetryMetrics;
use thiserror::Error;
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait CrawlAcquirer: Send + Sync {
    async fn fetch(
        &self,
        target: CrawlUrl,
        cancellation: &CancellationToken,
    ) -> Result<CrawlResponse, CrawlError>;
}

/// Resolves a country from already safety-validated public crawl addresses without network I/O.
pub trait CountryEnricher: Send + Sync {
    fn observe(&self, addresses: &[IpAddr]) -> Option<CountryObservation>;
}

pub struct NoCountryEnricher;

impl CountryEnricher for NoCountryEnricher {
    fn observe(&self, _: &[IpAddr]) -> Option<CountryObservation> {
        None
    }
}

#[async_trait]
impl CrawlAcquirer for Crawler {
    async fn fetch(
        &self,
        target: CrawlUrl,
        cancellation: &CancellationToken,
    ) -> Result<CrawlResponse, CrawlError> {
        self.fetch(target, cancellation).await
    }
}

/// Executes one queue delivery without acknowledging it; the caller acknowledges only after success.
pub struct WorkerProcessor {
    repository: Arc<dyn CrawlWorkerRepository>,
    acquirer: Arc<dyn CrawlAcquirer>,
    politeness_gate: Arc<dyn PolitenessGate>,
    artifact_store: Arc<dyn RawArtifactStore>,
    politeness_delay: Duration,
    artifact_retention: time::Duration,
    scheduler_settings: SchedulerSettings,
    country_enricher: Arc<dyn CountryEnricher>,
    metrics: Option<Arc<TelemetryMetrics>>,
}

/// Replays one published rule over immutable snapshots without changing the crawl projection.
pub struct ReprocessingProcessor {
    repository: Arc<dyn ReprocessingRepository>,
    artifact_store: Arc<dyn RawArtifactStore>,
    max_attempts: u16,
    retry_delay: time::Duration,
}

impl ReprocessingProcessor {
    pub fn new(
        repository: Arc<dyn ReprocessingRepository>,
        artifact_store: Arc<dyn RawArtifactStore>,
        max_attempts: u16,
        retry_delay: time::Duration,
    ) -> Self {
        Self {
            repository,
            artifact_store,
            max_attempts,
            retry_delay,
        }
    }

    /// Returns whether an item was claimed, so callers can wait efficiently when idle.
    pub async fn process_next(&self) -> Result<bool, ReprocessingProcessError> {
        let now = OffsetDateTime::now_utc();
        let Some(item) = self
            .repository
            .claim_next_reprocessing_item(now, self.max_attempts)
            .await?
        else {
            return Ok(false);
        };
        let html = match &item.artifact {
            Some(artifact) => match self.artifact_store.retrieve(artifact).await {
                Ok(contents) => ArtifactInput::Captured(contents),
                Err(error) => return self.record_failure(&item, error.to_string()).await,
            },
            None => ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
        };
        let parsed = parse(&ArtifactBundle {
            html,
            headers: ArtifactInput::Captured(item.response_headers.clone()),
            dns: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
            tls: ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
        });
        match evaluate_reprocessing(&parsed, &item.rule) {
            Ok(evaluation) => {
                self.repository
                    .record_reprocessing_success(&item, evaluation, OffsetDateTime::now_utc())
                    .await?;
                Ok(true)
            }
            Err(error) => self.record_failure(&item, error.to_string()).await,
        }
    }

    async fn record_failure(
        &self,
        item: &techatlas_models::ReprocessingWorkItem,
        summary: String,
    ) -> Result<bool, ReprocessingProcessError> {
        let now = OffsetDateTime::now_utc();
        let terminal = item.attempt_count >= self.max_attempts;
        self.repository
            .record_reprocessing_failure(
                item.id,
                &summary,
                now.saturating_add(self.retry_delay),
                terminal,
                now,
            )
            .await?;
        Ok(true)
    }
}

impl WorkerProcessor {
    pub fn new(
        repository: Arc<dyn CrawlWorkerRepository>,
        acquirer: Arc<dyn CrawlAcquirer>,
        politeness_gate: Arc<dyn PolitenessGate>,
        artifact_store: Arc<dyn RawArtifactStore>,
        politeness_delay: Duration,
        artifact_retention: time::Duration,
        scheduler_settings: SchedulerSettings,
    ) -> Self {
        Self {
            repository,
            acquirer,
            politeness_gate,
            artifact_store,
            politeness_delay,
            artifact_retention,
            scheduler_settings,
            country_enricher: Arc::new(NoCountryEnricher),
            metrics: None,
        }
    }

    pub fn with_country_enricher(mut self, country_enricher: Arc<dyn CountryEnricher>) -> Self {
        self.country_enricher = country_enricher;
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<TelemetryMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub async fn process(
        &self,
        job: &techatlas_models::CrawlJobV1,
        cancellation: &CancellationToken,
    ) -> Result<(), WorkerProcessError> {
        let claim = self
            .repository
            .claim_attempt(job, OffsetDateTime::now_utc())
            .await?;
        let ClaimedCrawlAttempt::Ready { canonical_domain } = claim else {
            return Ok(());
        };

        let target = match CrawlUrl::parse(canonical_domain.as_str()) {
            Ok(target) => target,
            Err(error) => {
                return self
                    .record_failure(job, "invalid_target", error.to_string())
                    .await;
            }
        };
        let politeness = PolitenessRequest {
            domain: target.host().to_owned(),
            minimum_delay: self.politeness_delay,
        };
        if let Err(error) = self
            .politeness_gate
            .acquire(&politeness, cancellation)
            .await
        {
            return self
                .record_failure(job, failure_code_for_politeness(&error), error.to_string())
                .await;
        }

        let response = match self.acquirer.fetch(target.clone(), cancellation).await {
            Ok(response) => response,
            Err(error) => {
                return self
                    .record_failure(job, failure_code_for_crawl(&error), error.to_string())
                    .await;
            }
        };
        let captured_at = OffsetDateTime::now_utc();
        let active_rules = self.repository.active_detection_rules().await?;
        let prepared = snapshot_from_response_with_metrics(
            &target,
            response,
            captured_at,
            self.country_enricher.as_ref(),
            self.metrics.as_deref(),
            &active_rules,
        )?;
        let artifact = match prepared.persistable_html {
            Some(contents) => {
                let retention_expires_at = captured_at.saturating_add(self.artifact_retention);
                match self
                    .artifact_store
                    .store(contents, retention_expires_at)
                    .await
                {
                    Ok(artifact) => Some(artifact),
                    Err(_) => {
                        return self
                            .record_failure(
                                job,
                                "artifact_storage_failed",
                                "raw artifact storage failed".to_owned(),
                            )
                            .await;
                    }
                }
            }
            None => None,
        };
        self.repository
            .record_success(
                job,
                SuccessfulCrawlRecord {
                    snapshot: prepared.snapshot,
                    artifact,
                    dns_observation: prepared.dns_observation,
                    tls_observation: prepared.tls_observation,
                    detections: prepared.detections,
                    observations: prepared.observations,
                },
            )
            .await?;
        Ok(())
    }

    async fn record_failure(
        &self,
        job: &techatlas_models::CrawlJobV1,
        code: &str,
        summary: String,
    ) -> Result<(), WorkerProcessError> {
        let failure = CrawlFailure::new(code, &bounded_summary(&summary))?;
        self.repository
            .record_failure(
                job,
                failure,
                OffsetDateTime::now_utc(),
                self.scheduler_settings,
            )
            .await?;
        Ok(())
    }
}

fn bounded_summary(summary: &str) -> String {
    let mut bounded = String::new();
    for character in summary.trim().chars() {
        if bounded.len().saturating_add(character.len_utf8()) > 1_024 {
            break;
        }
        bounded.push(character);
    }
    if bounded.is_empty() {
        "crawl processing failed".to_owned()
    } else {
        bounded
    }
}

pub async fn process_and_acknowledge<Q>(
    queue: &Q,
    processor: &WorkerProcessor,
    received: ReceivedCrawlJob,
    cancellation: &CancellationToken,
) -> Result<(), WorkerRuntimeError>
where
    Q: CrawlJobConsumer,
{
    processor.process(received.job(), cancellation).await?;
    queue.acknowledge(received.receipt()).await?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum ReprocessingProcessError {
    #[error("reprocessing persistence is unavailable")]
    Repository(#[from] ReprocessingRepositoryError),
}

pub async fn receive_and_process_once<Q>(
    queue: &Q,
    consumer: &CrawlJobConsumerName,
    processor: &WorkerProcessor,
    cancellation: &CancellationToken,
) -> Result<usize, WorkerRuntimeError>
where
    Q: CrawlJobConsumer,
{
    let mut deliveries = queue.reclaim_stale(consumer).await?;
    deliveries.extend(queue.receive(consumer).await?);
    let count = deliveries.len();
    for delivery in deliveries {
        process_and_acknowledge(queue, processor, delivery, cancellation).await?;
    }
    Ok(count)
}

fn snapshot_from_response_with_metrics(
    requested: &CrawlUrl,
    response: CrawlResponse,
    captured_at: OffsetDateTime,
    country_enricher: &dyn CountryEnricher,
    metrics: Option<&TelemetryMetrics>,
    active_rules: &[techatlas_models::ActiveDetectionRule],
) -> Result<PreparedSnapshot, WorkerProcessError> {
    let country_observation = country_enricher.observe(&response.resolved_addresses);
    let redirects = response
        .redirects
        .iter()
        .map(redirect_metadata)
        .collect::<Result<Vec<_>, _>>()?;
    let body_size_bytes =
        u64::try_from(response.body.len()).map_err(|_| WorkerProcessError::BodySize)?;
    let persistable_html = sanitized_html_for_storage(&response);
    let observation_started = std::time::Instant::now();
    let dns_observation = dns_observation(&response.dns, captured_at)?;
    let tls_observation = tls_observation(&response.tls, captured_at)?;
    if let Some(metrics) = metrics {
        metrics.record_operation(
            "dns_capture",
            if dns_observation.unavailable_reason().is_some() {
                "unavailable"
            } else {
                "success"
            },
            observation_started.elapsed(),
        );
        metrics.record_operation(
            "tls_capture",
            if tls_observation.unavailable_reason().is_some() {
                "unavailable"
            } else {
                "success"
            },
            observation_started.elapsed(),
        );
    }
    let parsed = parse(&ArtifactBundle {
        html: match &persistable_html {
            Some(html) => ArtifactInput::Captured(html.clone()),
            None => ArtifactInput::Unavailable(UnavailableReason::NotCaptured),
        },
        headers: ArtifactInput::Captured(response.headers.clone()),
        dns: parser_dns(&response.dns),
        tls: parser_tls(&response.tls),
    });
    let detection_started = std::time::Instant::now();
    let evaluated = evaluate_active(&parsed, active_rules).map_err(WorkerProcessError::Detection);
    if let Some(metrics) = metrics {
        metrics.record_operation(
            "detector_evaluation",
            if evaluated.is_ok() {
                "success"
            } else {
                "error"
            },
            detection_started.elapsed(),
        );
    }
    let (detections, observations) = evaluated?.into_parts();
    let snapshot = CrawlSnapshotMetadata::new(
        &requested.to_string(),
        &response.final_url.to_string(),
        redirects,
        response.status,
        response.headers,
        body_size_bytes,
        captured_at,
    )
    .map_err(WorkerProcessError::Snapshot)?
    .with_country_observation(country_observation);
    Ok(PreparedSnapshot {
        snapshot,
        persistable_html,
        dns_observation,
        tls_observation,
        detections,
        observations,
    })
}

struct PreparedSnapshot {
    snapshot: CrawlSnapshotMetadata,
    persistable_html: Option<Vec<u8>>,
    dns_observation: CrawlDnsObservation,
    tls_observation: CrawlTlsObservation,
    detections: Vec<techatlas_models::Detection>,
    observations: Vec<techatlas_models::RuleObservation>,
}

fn dns_observation(
    capture: &Capture<DnsCapture>,
    observed_at: OffsetDateTime,
) -> Result<CrawlDnsObservation, CrawlObservationError> {
    match capture {
        Capture::Captured(capture) => CrawlDnsObservation::available(
            "crawler_dns_v1",
            observed_at,
            &capture.queried_name,
            capture.addresses.clone(),
        ),
        Capture::Unavailable(reason) => CrawlDnsObservation::unavailable(
            "crawler_dns_v1",
            observed_at,
            observation_reason(*reason),
        ),
    }
}

fn tls_observation(
    capture: &Capture<TlsCapture>,
    observed_at: OffsetDateTime,
) -> Result<CrawlTlsObservation, CrawlObservationError> {
    match capture {
        Capture::Captured(capture) => CrawlTlsObservation::available(
            "crawler_tls_v1",
            observed_at,
            match capture.validation {
                TlsValidation::Verified => TlsCertificateValidation::Verified,
                TlsValidation::Failed => TlsCertificateValidation::Failed,
            },
            capture.protocol.clone(),
            capture.cipher_suite.clone(),
            capture.certificate_subject.clone(),
            capture.certificate_issuer.clone(),
            capture.subject_alternative_names.clone(),
            capture.certificate_not_before,
            capture.certificate_not_after,
        ),
        Capture::Unavailable(reason) => CrawlTlsObservation::unavailable(
            "crawler_tls_v1",
            observed_at,
            observation_reason(*reason),
        ),
    }
}

fn parser_dns(capture: &Capture<DnsCapture>) -> ArtifactInput<techatlas_parser::DnsArtifact> {
    match capture {
        Capture::Captured(capture) => ArtifactInput::Captured(techatlas_parser::DnsArtifact {
            queried_name: capture.queried_name.clone(),
            addresses: capture.addresses.clone(),
        }),
        Capture::Unavailable(reason) => ArtifactInput::Unavailable(parser_reason(*reason)),
    }
}

fn parser_tls(capture: &Capture<TlsCapture>) -> ArtifactInput<techatlas_parser::TlsArtifact> {
    match capture {
        Capture::Captured(capture) => ArtifactInput::Captured(techatlas_parser::TlsArtifact {
            protocol: capture.protocol.clone(),
            cipher_suite: capture.cipher_suite.clone(),
            certificate_subject: capture.certificate_subject.clone(),
            certificate_issuer: capture.certificate_issuer.clone(),
            subject_alternative_names: capture.subject_alternative_names.clone(),
        }),
        Capture::Unavailable(reason) => ArtifactInput::Unavailable(parser_reason(*reason)),
    }
}

fn observation_reason(reason: CaptureUnavailableReason) -> CrawlObservationUnavailableReason {
    match reason {
        CaptureUnavailableReason::NotCaptured => CrawlObservationUnavailableReason::NotCaptured,
        CaptureUnavailableReason::NotApplicable => CrawlObservationUnavailableReason::NotApplicable,
        CaptureUnavailableReason::Timeout => CrawlObservationUnavailableReason::Timeout,
        CaptureUnavailableReason::ConnectionFailed => {
            CrawlObservationUnavailableReason::ConnectionFailed
        }
        CaptureUnavailableReason::Invalid => CrawlObservationUnavailableReason::Invalid,
    }
}

fn parser_reason(reason: CaptureUnavailableReason) -> UnavailableReason {
    match reason {
        CaptureUnavailableReason::NotCaptured => UnavailableReason::NotCaptured,
        CaptureUnavailableReason::NotApplicable => UnavailableReason::Unsupported,
        CaptureUnavailableReason::Invalid => UnavailableReason::Invalid,
        CaptureUnavailableReason::Timeout | CaptureUnavailableReason::ConnectionFailed => {
            UnavailableReason::NotCaptured
        }
    }
}

fn redirect_metadata(
    redirect: &CrawlRedirect,
) -> Result<CrawlRedirectMetadata, CrawlSnapshotMetadataError> {
    CrawlRedirectMetadata::new(
        &redirect.from.to_string(),
        &redirect.to.to_string(),
        redirect.status,
    )
}

fn failure_code_for_politeness(error: &PolitenessGateError) -> &'static str {
    match error {
        PolitenessGateError::Cancelled => "cancelled",
        PolitenessGateError::Unavailable { .. } => "politeness_unavailable",
    }
}

fn failure_code_for_crawl(error: &CrawlError) -> &'static str {
    match error {
        CrawlError::Cancelled => "cancelled",
        CrawlError::Timeout => "network_timeout",
        CrawlError::Dns(techatlas_crawler::DnsSafetyError::BlockedAddress { .. }) => {
            "unsafe_target"
        }
        CrawlError::Dns(techatlas_crawler::DnsSafetyError::Timeout) => "dns_timeout",
        CrawlError::Dns(techatlas_crawler::DnsSafetyError::Resolution(_)) => "dns_failure",
        CrawlError::Transport(_) => "network_failure",
        CrawlError::ResponseTooLarge { .. } => "body_limit_exceeded",
        CrawlError::RedirectLimitExceeded { .. } => "redirect_limit_exceeded",
        CrawlError::MissingRedirectLocation | CrawlError::InvalidRedirectTarget { .. } => {
            "invalid_redirect"
        }
        CrawlError::RobotsDenied { .. } => "robots_denied",
        CrawlError::RobotsUnavailable { .. } => "robots_unavailable",
    }
}

#[derive(Debug, Error)]
pub enum WorkerProcessError {
    #[error(transparent)]
    Repository(#[from] CrawlWorkerRepositoryError),
    #[error(transparent)]
    Failure(#[from] CrawlFailureError),
    #[error(transparent)]
    Snapshot(#[from] CrawlSnapshotMetadataError),
    #[error(transparent)]
    Detection(#[from] DetectorError),
    #[error(transparent)]
    Observation(#[from] CrawlObservationError),
    #[error("response body size cannot be represented")]
    BodySize,
}

#[derive(Debug, Error)]
pub enum WorkerRuntimeError {
    #[error(transparent)]
    Queue(#[from] CrawlJobQueueError),
    #[error(transparent)]
    Processing(#[from] WorkerProcessError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Mutex};
    use techatlas_models::{CorrelationId, CrawlJobId, DomainId, IdempotencyKey};
    use techatlas_storage::{RawArtifactStore, RawArtifactStoreError};
    use uuid::Uuid;

    struct FakeRepository {
        claim: ClaimedCrawlAttempt,
        successes: Mutex<usize>,
        failures: Mutex<Vec<String>>,
    }

    struct FakeReprocessingRepository {
        item: Mutex<Option<techatlas_models::ReprocessingWorkItem>>,
        evaluations: Mutex<Vec<techatlas_models::ReprocessingEvaluation>>,
        failures: Mutex<usize>,
    }

    #[async_trait]
    impl ReprocessingRepository for FakeReprocessingRepository {
        async fn claim_next_reprocessing_item(
            &self,
            _: OffsetDateTime,
            _: u16,
        ) -> Result<Option<techatlas_models::ReprocessingWorkItem>, ReprocessingRepositoryError>
        {
            Ok(self
                .item
                .lock()
                .expect("test lock should not be poisoned")
                .take())
        }

        async fn record_reprocessing_success(
            &self,
            _: &techatlas_models::ReprocessingWorkItem,
            evaluation: techatlas_models::ReprocessingEvaluation,
            _: OffsetDateTime,
        ) -> Result<(), ReprocessingRepositoryError> {
            self.evaluations
                .lock()
                .expect("test lock should not be poisoned")
                .push(evaluation);
            Ok(())
        }

        async fn record_reprocessing_failure(
            &self,
            _: Uuid,
            _: &str,
            _: OffsetDateTime,
            _: bool,
            _: OffsetDateTime,
        ) -> Result<(), ReprocessingRepositoryError> {
            *self
                .failures
                .lock()
                .expect("test lock should not be poisoned") += 1;
            Ok(())
        }
    }

    #[async_trait]
    impl CrawlWorkerRepository for FakeRepository {
        async fn active_detection_rules(
            &self,
        ) -> Result<Vec<techatlas_models::ActiveDetectionRule>, CrawlWorkerRepositoryError>
        {
            Ok(Vec::new())
        }
        async fn claim_attempt(
            &self,
            _: &techatlas_models::CrawlJobV1,
            _: OffsetDateTime,
        ) -> Result<ClaimedCrawlAttempt, CrawlWorkerRepositoryError> {
            Ok(self.claim.clone())
        }

        async fn record_success(
            &self,
            _: &techatlas_models::CrawlJobV1,
            _: SuccessfulCrawlRecord,
        ) -> Result<(), CrawlWorkerRepositoryError> {
            *self
                .successes
                .lock()
                .expect("test lock should not be poisoned") += 1;
            Ok(())
        }

        async fn record_failure(
            &self,
            _: &techatlas_models::CrawlJobV1,
            failure: CrawlFailure,
            _: OffsetDateTime,
            _: SchedulerSettings,
        ) -> Result<(), CrawlWorkerRepositoryError> {
            self.failures
                .lock()
                .expect("test lock should not be poisoned")
                .push(failure.code().to_owned());
            Ok(())
        }
    }

    enum FakeAcquisition {
        Success(Box<CrawlResponse>),
        Cancelled,
    }

    struct FakeAcquirer {
        result: FakeAcquisition,
    }

    #[async_trait]
    impl CrawlAcquirer for FakeAcquirer {
        async fn fetch(
            &self,
            _: CrawlUrl,
            _: &CancellationToken,
        ) -> Result<CrawlResponse, CrawlError> {
            match &self.result {
                FakeAcquisition::Success(response) => Ok((**response).clone()),
                FakeAcquisition::Cancelled => Err(CrawlError::Cancelled),
            }
        }
    }

    struct OpenGate;

    #[async_trait]
    impl PolitenessGate for OpenGate {
        async fn acquire(
            &self,
            _: &PolitenessRequest,
            _: &CancellationToken,
        ) -> Result<(), PolitenessGateError> {
            Ok(())
        }
    }

    struct MemoryArtifactStore;

    #[async_trait]
    impl RawArtifactStore for MemoryArtifactStore {
        async fn store(
            &self,
            contents: Vec<u8>,
            retention_expires_at: OffsetDateTime,
        ) -> Result<techatlas_models::RawArtifactMetadata, RawArtifactStoreError> {
            let checksum = "a".repeat(64);
            techatlas_models::RawArtifactMetadata::response_body(
                "sha256/aa/fixture.zst",
                &checksum,
                techatlas_models::RawArtifactCompression::Zstd,
                u64::try_from(contents.len()).map_err(|_| RawArtifactStoreError::Size)?,
                1,
                retention_expires_at,
            )
            .map_err(|_| RawArtifactStoreError::Integrity)
        }

        async fn retrieve(
            &self,
            _: &techatlas_models::RawArtifactMetadata,
        ) -> Result<Vec<u8>, RawArtifactStoreError> {
            Ok(Vec::new())
        }
    }

    struct FailingArtifactStore;

    #[async_trait]
    impl RawArtifactStore for FailingArtifactStore {
        async fn store(
            &self,
            _: Vec<u8>,
            _: OffsetDateTime,
        ) -> Result<techatlas_models::RawArtifactMetadata, RawArtifactStoreError> {
            Err(RawArtifactStoreError::Unavailable)
        }

        async fn retrieve(
            &self,
            _: &techatlas_models::RawArtifactMetadata,
        ) -> Result<Vec<u8>, RawArtifactStoreError> {
            Err(RawArtifactStoreError::Unavailable)
        }
    }

    fn job() -> techatlas_models::CrawlJobV1 {
        techatlas_models::CrawlJobV1::new(
            CrawlJobId::from_uuid(Uuid::from_u128(1)),
            DomainId::from_uuid(Uuid::from_u128(2)),
            CorrelationId::from_uuid(Uuid::from_u128(3)),
            IdempotencyKey::parse("crawl:1").expect("test key should be valid"),
        )
    }

    #[tokio::test]
    async fn reprocessing_persists_a_deterministic_version_capture_without_crawling() {
        let repository = Arc::new(FakeReprocessingRepository {
            item: Mutex::new(Some(techatlas_models::ReprocessingWorkItem {
                id: Uuid::from_u128(1),
                run_id: Uuid::from_u128(2),
                snapshot_id: Uuid::from_u128(3),
                domain_id: Uuid::from_u128(4),
                technology_id: Uuid::from_u128(5),
                detection_rule_version_id: Uuid::from_u128(6),
                captured_at: OffsetDateTime::UNIX_EPOCH,
                attempt_count: 1,
                response_headers: BTreeMap::from([(
                    "x-powered-by".to_owned(),
                    "Next.js/14.2.1".to_owned(),
                )]),
                artifact: None,
                rule: techatlas_models::ActiveDetectionRule {
                    technology_slug: "nextjs".to_owned(),
                    technology_category_slug: "framework".to_owned(),
                    rule_slug: "nextjs-v1".to_owned(),
                    version: 2,
                    definition: serde_json::json!({
                        "threshold": 70,
                        "signals": [{"source":"header", "key":"x-powered-by", "match":"contains_ignore_case", "value":"next", "weight":90}],
                        "version_evidence": {"source":"header", "key":"x-powered-by", "pattern":"(?i)next\\.js/([0-9.]+)"}
                    }),
                },
            })),
            evaluations: Mutex::new(Vec::new()),
            failures: Mutex::new(0),
        });
        let processor = ReprocessingProcessor::new(
            repository.clone(),
            Arc::new(MemoryArtifactStore),
            3,
            time::Duration::seconds(1),
        );

        assert!(
            processor
                .process_next()
                .await
                .expect("replay should succeed")
        );
        let evaluations = repository
            .evaluations
            .lock()
            .expect("test lock should not be poisoned");
        assert_eq!(evaluations.len(), 1);
        assert_eq!(evaluations[0].technology_version.as_deref(), Some("14.2.1"));
        assert_eq!(
            *repository
                .failures
                .lock()
                .expect("test lock should not be poisoned"),
            0
        );
    }

    #[tokio::test]
    async fn successful_delivery_creates_one_snapshot_metadata_record() {
        let repository = Arc::new(FakeRepository {
            claim: ClaimedCrawlAttempt::Ready {
                canonical_domain: techatlas_models::CanonicalDomain::parse("example.com")
                    .expect("test domain should be valid"),
            },
            successes: Mutex::new(0),
            failures: Mutex::new(Vec::new()),
        });
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("https://example.com/").expect("test URL should be valid"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::new(),
            body: b"fixture".to_vec(),
            dns: Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        };
        let processor = WorkerProcessor::new(
            repository.clone(),
            Arc::new(FakeAcquirer {
                result: FakeAcquisition::Success(Box::new(response)),
            }),
            Arc::new(OpenGate),
            Arc::new(MemoryArtifactStore),
            Duration::from_secs(1),
            time::Duration::days(30),
            SchedulerSettings::default(),
        );

        processor
            .process(&job(), &CancellationToken::new())
            .await
            .expect("fixture crawl should succeed");

        assert_eq!(
            *repository
                .successes
                .lock()
                .expect("test lock should not be poisoned"),
            1
        );
        assert!(
            repository
                .failures
                .lock()
                .expect("test lock should not be poisoned")
                .is_empty()
        );
    }

    #[test]
    fn captured_dns_and_validation_failed_tls_are_preserved_for_snapshots() {
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("https://example.com/").expect("test URL should be valid"),
            resolved_addresses: vec![
                "93.184.216.34"
                    .parse()
                    .expect("test address should be valid"),
            ],
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::new(),
            body: Vec::new(),
            dns: Capture::Captured(DnsCapture {
                queried_name: "example.com".to_owned(),
                addresses: vec![
                    "93.184.216.34"
                        .parse()
                        .expect("test address should be valid"),
                ],
            }),
            tls: Capture::Captured(TlsCapture {
                validation: TlsValidation::Failed,
                protocol: Some("TLS 1.3".to_owned()),
                cipher_suite: Some("TLS13_AES_256_GCM_SHA384".to_owned()),
                certificate_subject: Some("example.com".to_owned()),
                certificate_issuer: Some("Example Certificate Authority".to_owned()),
                subject_alternative_names: vec!["example.com".to_owned()],
                certificate_not_before: None,
                certificate_not_after: None,
            }),
        };

        let prepared = snapshot_from_response_with_metrics(
            &CrawlUrl::parse("https://example.com/").expect("test URL should be valid"),
            response,
            OffsetDateTime::UNIX_EPOCH,
            &NoCountryEnricher,
            None,
            &[],
        )
        .expect("captured optional facts should not prevent a snapshot");

        assert_eq!(prepared.dns_observation.queried_name(), Some("example.com"));
        assert_eq!(
            prepared.tls_observation.validation(),
            Some(TlsCertificateValidation::Failed)
        );
        assert_eq!(
            prepared.tls_observation.certificate_subject(),
            Some("example.com")
        );
    }

    #[test]
    fn unavailable_optional_facts_do_not_prevent_a_successful_snapshot() {
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("http://example.com/").expect("test URL should be valid"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::new(),
            body: Vec::new(),
            dns: Capture::Unavailable(CaptureUnavailableReason::Timeout),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        };

        let prepared = snapshot_from_response_with_metrics(
            &CrawlUrl::parse("http://example.com/").expect("test URL should be valid"),
            response,
            OffsetDateTime::UNIX_EPOCH,
            &NoCountryEnricher,
            None,
            &[],
        )
        .expect("unavailable optional facts should not prevent a snapshot");

        assert_eq!(
            prepared.dns_observation.unavailable_reason(),
            Some(CrawlObservationUnavailableReason::Timeout)
        );
        assert_eq!(
            prepared.tls_observation.unavailable_reason(),
            Some(CrawlObservationUnavailableReason::NotApplicable)
        );
    }

    #[tokio::test]
    async fn terminal_delivery_is_idempotent() {
        let repository = Arc::new(FakeRepository {
            claim: ClaimedCrawlAttempt::Terminal,
            successes: Mutex::new(0),
            failures: Mutex::new(Vec::new()),
        });
        let processor = WorkerProcessor::new(
            repository.clone(),
            Arc::new(FakeAcquirer {
                result: FakeAcquisition::Cancelled,
            }),
            Arc::new(OpenGate),
            Arc::new(MemoryArtifactStore),
            Duration::from_secs(1),
            time::Duration::days(30),
            SchedulerSettings::default(),
        );

        processor
            .process(&job(), &CancellationToken::new())
            .await
            .expect("terminal delivery should be acknowledged without crawling");

        assert_eq!(
            *repository
                .successes
                .lock()
                .expect("test lock should not be poisoned"),
            0
        );
        assert!(
            repository
                .failures
                .lock()
                .expect("test lock should not be poisoned")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn artifact_storage_failure_records_a_retryable_attempt_failure() {
        let repository = Arc::new(FakeRepository {
            claim: ClaimedCrawlAttempt::Ready {
                canonical_domain: techatlas_models::CanonicalDomain::parse("example.com")
                    .expect("test domain should be valid"),
            },
            successes: Mutex::new(0),
            failures: Mutex::new(Vec::new()),
        });
        let response = CrawlResponse {
            final_url: CrawlUrl::parse("https://example.com/").expect("test URL should be valid"),
            resolved_addresses: Vec::new(),
            redirects: Vec::new(),
            status: 200,
            headers: BTreeMap::from([("content-type".to_owned(), "text/html".to_owned())]),
            body: b"<html>fixture</html>".to_vec(),
            dns: Capture::Unavailable(CaptureUnavailableReason::ConnectionFailed),
            tls: Capture::Unavailable(CaptureUnavailableReason::NotApplicable),
        };
        let processor = WorkerProcessor::new(
            repository.clone(),
            Arc::new(FakeAcquirer {
                result: FakeAcquisition::Success(Box::new(response)),
            }),
            Arc::new(OpenGate),
            Arc::new(FailingArtifactStore),
            Duration::from_secs(1),
            time::Duration::days(30),
            SchedulerSettings::default(),
        );

        processor
            .process(&job(), &CancellationToken::new())
            .await
            .expect("storage failure should be recorded");

        assert_eq!(
            repository
                .failures
                .lock()
                .expect("test lock should not be poisoned")
                .as_slice(),
            ["artifact_storage_failed"]
        );
        assert_eq!(
            *repository
                .successes
                .lock()
                .expect("test lock should not be poisoned"),
            0
        );
    }
}
