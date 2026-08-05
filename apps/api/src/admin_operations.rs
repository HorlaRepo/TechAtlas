use crate::{
    admin_auth::AdminPermission,
    admin_domains::{AdminApiError, ErrorEnvelope, authorize},
    app::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::HeaderMap,
    routing::{get, patch, post},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use techatlas_models::{
    AdminAuditCursor, AdminCrawlPolicy, AdminCrawlRetryState, AdminOperationError, CanonicalDomain,
    CrawlPriority, CsvDomainImport,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::{IntoParams, ToSchema};

const MAX_IMPORT_BYTES: usize = 1_048_576;
const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/operations/overview",
            get(operations_overview),
        )
        .route("/api/v1/admin/imports/csv", post(import_csv))
        .route(
            "/api/v1/admin/domains/{canonical_domain}/policy",
            patch(update_policy),
        )
        .route(
            "/api/v1/admin/domains/{canonical_domain}/crawl",
            post(request_domain_crawl),
        )
        .route("/api/v1/admin/audit-events", get(audit_events))
        .route("/api/v1/admin/crawl-attempts", get(crawl_attempts))
        .route(
            "/api/v1/admin/crawl-attempts/{job_id}/retry",
            post(retry_crawl_attempt),
        )
        .route(
            "/api/v1/admin/country-enrichment/recrawl",
            post(schedule_country_enrichment_recrawl),
        )
        .route("/api/v1/admin/detection-rules", get(detection_rules))
        .route(
            "/api/v1/admin/detection-rules/{rule_slug}/draft",
            patch(save_rule_draft),
        )
        .route(
            "/api/v1/admin/detection-rules/{rule_slug}/draft/test",
            post(test_rule_draft),
        )
        .route(
            "/api/v1/admin/detection-rules/{rule_slug}/draft/publish",
            post(publish_rule_draft),
        )
        .route(
            "/api/v1/admin/detection-rules/{rule_slug}/active-version",
            patch(set_active_rule_version),
        )
        .route(
            "/api/v1/admin/detection-rules/{rule_slug}/versions/{rule_version}/reprocessing",
            post(start_reprocessing),
        )
        .route(
            "/api/v1/admin/detection-reprocessing-runs",
            get(reprocessing_runs),
        )
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CsvImportRequest {
    pub source_name: String,
    pub csv: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CsvImportResponse {
    pub import_id: String,
    pub source_name: String,
    pub accepted_row_count: u32,
    pub duplicate_row_count: u32,
    pub rejected_row_count: u32,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePolicyRequest {
    pub is_enabled: bool,
    pub priority: String,
    pub desired_interval_hours: u16,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PolicyResponse {
    pub is_enabled: bool,
    pub priority: String,
    pub desired_interval_hours: u16,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrawlRequestResponse {
    pub canonical_domain: String,
    pub scheduled_at: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AuditQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEventResponse {
    pub id: String,
    pub actor_subject: String,
    pub action: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub occurred_at: String,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEventPageResponse {
    pub events: Vec<AuditEventResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CrawlAttemptQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrawlAttemptResponse {
    pub job_id: String,
    pub canonical_domain: String,
    pub attempt_number: u16,
    pub status: String,
    pub queued_at: String,
    pub finished_at: Option<String>,
    pub failure_code: Option<String>,
    pub failure_summary: Option<String>,
    pub retry_state: AdminCrawlRetryState,
    pub retry_eligible: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CrawlAttemptPageResponse {
    pub attempts: Vec<CrawlAttemptResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CountryEnrichmentRecrawlResponse {
    pub scheduled_domain_count: u64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RuleDraftRequest {
    /// A deterministic rule definition encoded as a JSON object.
    pub definition: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RuleFixtureSignal {
    pub source: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RuleFixtureTestRequest {
    pub signals: Vec<RuleFixtureSignal>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RuleFixtureTestResponse {
    pub passed: bool,
    pub confidence: u8,
    pub threshold: u8,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ActiveRuleVersionRequest {
    pub version: Option<u16>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct StartReprocessingRequest {
    pub idempotency_key: String,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ReprocessingRunQuery {
    pub rule_slug: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReprocessingRunResponse {
    pub id: String,
    pub rule_slug: String,
    pub rule_version: u16,
    pub status: String,
    pub total_snapshot_count: u32,
    pub succeeded_snapshot_count: u32,
    pub failed_snapshot_count: u32,
    pub requested_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ReprocessingRunListResponse {
    pub runs: Vec<ReprocessingRunResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DetectionRuleVersionResponse {
    pub version: u16,
    pub definition: String,
    pub published_at: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DetectionRuleResponse {
    pub rule_slug: String,
    pub technology_slug: String,
    pub technology_name: String,
    pub active_version: Option<u16>,
    pub draft_definition: Option<String>,
    pub draft_updated_at: Option<String>,
    pub versions: Vec<DetectionRuleVersionResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DetectionRuleListResponse {
    pub rules: Vec<DetectionRuleResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OperationsOverviewResponse {
    pub generated_at: String,
    pub system_status: String,
    pub domain_count: u64,
    pub technology_count: u64,
    pub current_detection_count: u64,
    pub successful_crawl_count: u64,
    pub queue: QueueOverviewResponse,
    pub dependencies: Vec<DependencyStatusResponse>,
    pub alertable_failures: Vec<OperationsAlertResponse>,
    pub throughput: Vec<ThroughputPointResponse>,
    pub workers: Vec<WorkerOverviewResponse>,
    pub activity: Vec<OperationsActivityResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DependencyStatusResponse {
    pub name: String,
    pub status: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OperationsAlertResponse {
    pub code: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct QueueOverviewResponse {
    pub ready_count: u64,
    pub processing_count: u64,
    pub scheduled_count: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ThroughputPointResponse {
    pub observed_at: String,
    pub completed_count: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WorkerOverviewResponse {
    pub name: String,
    pub region: String,
    pub in_flight_work: u32,
    pub completed_total: u64,
    pub status: String,
    pub last_heartbeat_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OperationsActivityResponse {
    pub id: String,
    pub title: String,
    pub description: String,
    pub occurred_at: String,
    pub kind: String,
}

#[utoipa::path(get, path = "/api/v1/admin/operations/overview", tag = "admin-operations", responses((status = 200, body = OperationsOverviewResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn operations_overview(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OperationsOverviewResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let now = OffsetDateTime::now_utc();
    let (operations, (postgres, redis, meilisearch)) = tokio::join!(
        async {
            tokio::try_join!(
                state.admin_operations.overview(now),
                state.admin_queue.snapshot(),
            )
        },
        async {
            tokio::join!(
                state.postgres.check(),
                state.redis.check(),
                state.meilisearch.check(),
            )
        },
    );
    let (overview, queue) = operations.map_err(map_operation)?;
    let dependencies = [
        ("postgres", postgres),
        ("redis", redis),
        ("meilisearch", meilisearch),
    ]
    .into_iter()
    .map(|(name, status)| DependencyStatusResponse {
        name: name.to_owned(),
        status: if status.is_ready() {
            "ready".to_owned()
        } else {
            "unavailable".to_owned()
        },
    })
    .collect::<Vec<_>>();
    let workers = overview
        .workers
        .into_iter()
        .map(|worker| {
            let is_healthy = now - worker.last_heartbeat_at <= time::Duration::seconds(45);
            Ok(WorkerOverviewResponse {
                name: worker.name,
                region: worker.region,
                in_flight_work: worker.in_flight_work,
                completed_total: worker.completed_total,
                status: if is_healthy { "healthy" } else { "stale" }.to_owned(),
                last_heartbeat_at: worker
                    .last_heartbeat_at
                    .format(&Rfc3339)
                    .map_err(|_| AdminApiError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>, AdminApiError>>()?;
    let dependencies_ready = dependencies
        .iter()
        .all(|dependency| dependency.status == "ready");
    let system_status = if dependencies_ready
        && !workers.is_empty()
        && workers.iter().all(|worker| worker.status == "healthy")
    {
        "online"
    } else {
        "degraded"
    };
    let mut alertable_failures = dependencies
        .iter()
        .filter(|dependency| dependency.status != "ready")
        .map(|dependency| OperationsAlertResponse {
            code: "dependency_unavailable".to_owned(),
            severity: "critical".to_owned(),
            message: format!("{} is unavailable.", dependency.name),
        })
        .collect::<Vec<_>>();
    let stale_workers = workers
        .iter()
        .filter(|worker| worker.status == "stale")
        .count();
    if stale_workers > 0 {
        alertable_failures.push(OperationsAlertResponse {
            code: "worker_heartbeat_stale".to_owned(),
            severity: "warning".to_owned(),
            message: format!(
                "{stale_workers} worker heartbeat{} stale.",
                if stale_workers == 1 { " is" } else { "s are" }
            ),
        });
    }
    let throughput = overview
        .throughput
        .into_iter()
        .map(|point| {
            Ok(ThroughputPointResponse {
                observed_at: point
                    .observed_at
                    .format(&Rfc3339)
                    .map_err(|_| AdminApiError::Unavailable)?,
                completed_count: point.completed_count,
            })
        })
        .collect::<Result<Vec<_>, AdminApiError>>()?;
    let activity = overview
        .activity
        .into_iter()
        .map(|event| {
            Ok(OperationsActivityResponse {
                id: event.id,
                title: event.title,
                description: event.description,
                occurred_at: event
                    .occurred_at
                    .format(&Rfc3339)
                    .map_err(|_| AdminApiError::Unavailable)?,
                kind: match event.kind {
                    techatlas_models::AdminActivityKind::Discovery => "discovery",
                    techatlas_models::AdminActivityKind::Success => "success",
                    techatlas_models::AdminActivityKind::System => "system",
                }
                .to_owned(),
            })
        })
        .collect::<Result<Vec<_>, AdminApiError>>()?;
    Ok(Json(OperationsOverviewResponse {
        generated_at: now
            .format(&Rfc3339)
            .map_err(|_| AdminApiError::Unavailable)?,
        system_status: system_status.to_owned(),
        domain_count: overview.domain_count,
        technology_count: overview.technology_count,
        current_detection_count: overview.current_detection_count,
        successful_crawl_count: overview.successful_crawl_count,
        queue: QueueOverviewResponse {
            ready_count: queue.ready_count,
            processing_count: queue.processing_count,
            scheduled_count: overview.scheduled_crawl_count,
        },
        dependencies,
        alertable_failures,
        throughput,
        workers,
        activity,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/imports/csv", tag = "admin-imports", request_body = CsvImportRequest, responses((status = 201, body = CsvImportResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn import_csv(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CsvImportRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<CsvImportResponse>), AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    if payload.csv.len() > MAX_IMPORT_BYTES {
        return Err(AdminApiError::Validation);
    }
    let document =
        CsvDomainImport::parse(payload.csv.as_bytes()).map_err(|_| AdminApiError::Validation)?;
    let result = state
        .admin_imports
        .import_csv(&payload.source_name, principal.subject(), document)
        .await
        .map_err(map_operation)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CsvImportResponse {
            import_id: result.import_id.to_string(),
            source_name: result.source_name,
            accepted_row_count: result.accepted_row_count,
            duplicate_row_count: result.duplicate_row_count,
            rejected_row_count: result.rejected_row_count,
        }),
    ))
}

#[utoipa::path(patch, path = "/api/v1/admin/domains/{canonical_domain}/policy", tag = "admin-domains", params(("canonical_domain" = String, Path)), request_body = UpdatePolicyRequest, responses((status = 200, body = PolicyResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn update_policy(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(canonical_domain): Path<String>,
    payload: Result<Json<UpdatePolicyRequest>, JsonRejection>,
) -> Result<Json<PolicyResponse>, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let domain =
        CanonicalDomain::parse(&canonical_domain).map_err(|_| AdminApiError::Validation)?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    let priority = match payload.priority.as_str() {
        "high" => CrawlPriority::High,
        "medium" => CrawlPriority::Medium,
        "low" => CrawlPriority::Low,
        _ => return Err(AdminApiError::Validation),
    };
    let policy =
        AdminCrawlPolicy::new(payload.is_enabled, priority, payload.desired_interval_hours)
            .map_err(map_operation)?;
    let policy = state
        .admin_policies
        .update_policy(&domain, policy, principal.subject())
        .await
        .map_err(map_operation)?;
    Ok(Json(PolicyResponse {
        is_enabled: policy.is_enabled,
        priority: policy.priority,
        desired_interval_hours: policy.desired_interval_hours,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/domains/{canonical_domain}/crawl", tag = "admin-domains", params(("canonical_domain" = String, Path, description = "Existing enabled domain name or normalizable HTTP(S) URL")), responses((status = 202, description = "Domain made eligible for scheduler processing", body = CrawlRequestResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 409, description = "Crawl policy is disabled", body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn request_domain_crawl(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(canonical_domain): Path<String>,
) -> Result<(axum::http::StatusCode, Json<CrawlRequestResponse>), AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let domain =
        CanonicalDomain::parse(&canonical_domain).map_err(|_| AdminApiError::Validation)?;
    let now = OffsetDateTime::now_utc();
    state
        .admin_scheduler
        .request_domain_crawl(&domain, principal.subject(), now)
        .await
        .map_err(map_operation)?;
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(CrawlRequestResponse {
            canonical_domain: domain.as_str().to_owned(),
            scheduled_at: now
                .format(&Rfc3339)
                .map_err(|_| AdminApiError::Unavailable)?,
        }),
    ))
}

#[utoipa::path(get, path = "/api/v1/admin/audit-events", tag = "admin-audit", params(AuditQuery), responses((status = 200, body = AuditEventPageResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn audit_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<AuditQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<AuditEventPageResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let Query(query) = query.map_err(|_| AdminApiError::Validation)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(AdminApiError::Validation);
    }
    let cursor = query
        .cursor
        .as_deref()
        .map(parse_audit_cursor)
        .transpose()?;
    let (events, next_cursor) = state
        .admin_audits
        .audit_events(cursor, limit)
        .await
        .map_err(map_operation)?;
    Ok(Json(AuditEventPageResponse {
        events: events
            .into_iter()
            .map(|event| AuditEventResponse {
                id: event.id,
                actor_subject: event.actor_subject,
                action: event.action,
                resource_kind: event.resource_kind,
                resource_id: event.resource_id,
                occurred_at: event.occurred_at.to_string(),
            })
            .collect(),
        next_cursor: next_cursor.map(format_audit_cursor).transpose()?,
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/crawl-attempts", tag = "admin-operations", params(CrawlAttemptQuery), responses((status = 200, body = CrawlAttemptPageResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn crawl_attempts(
    State(state): State<AppState>,
    headers: HeaderMap,
    query: Result<Query<CrawlAttemptQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<CrawlAttemptPageResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let Query(query) = query.map_err(|_| AdminApiError::Validation)?;
    let limit = query.limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(AdminApiError::Validation);
    }
    let attempts = state
        .admin_scheduler
        .crawl_attempts(limit)
        .await
        .map_err(map_operation)?;
    Ok(Json(CrawlAttemptPageResponse {
        attempts: attempts
            .into_iter()
            .map(crawl_attempt_response)
            .collect::<Result<_, _>>()?,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/crawl-attempts/{job_id}/retry", tag = "admin-operations", params(("job_id" = String, Path)), responses((status = 201, body = CrawlAttemptResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn retry_crawl_attempt(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(job_id): Path<String>,
) -> Result<(axum::http::StatusCode, Json<CrawlAttemptResponse>), AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let attempt = state
        .admin_scheduler
        .retry_terminal_attempt(&job_id, principal.subject(), OffsetDateTime::now_utc())
        .await
        .map_err(map_operation)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(crawl_attempt_response(attempt)?),
    ))
}

#[utoipa::path(post, path = "/api/v1/admin/country-enrichment/recrawl", tag = "admin-operations", responses((status = 200, body = CountryEnrichmentRecrawlResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn schedule_country_enrichment_recrawl(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CountryEnrichmentRecrawlResponse>, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let scheduled_domain_count = state
        .admin_scheduler
        .schedule_country_enrichment_recrawl(principal.subject(), OffsetDateTime::now_utc())
        .await
        .map_err(map_operation)?;
    Ok(Json(CountryEnrichmentRecrawlResponse {
        scheduled_domain_count,
    }))
}

#[utoipa::path(get, path = "/api/v1/admin/detection-rules", tag = "admin-rules", responses((status = 200, body = DetectionRuleListResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn detection_rules(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DetectionRuleListResponse>, AdminApiError> {
    authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let rules = state.admin_rules.rules().await.map_err(map_operation)?;
    Ok(Json(DetectionRuleListResponse {
        rules: rules
            .into_iter()
            .map(rule_response)
            .collect::<Result<_, _>>()?,
    }))
}

#[utoipa::path(patch, path = "/api/v1/admin/detection-rules/{rule_slug}/draft", tag = "admin-rules", params(("rule_slug" = String, Path)), request_body = RuleDraftRequest, responses((status = 200, body = DetectionRuleResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn save_rule_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(rule_slug): Path<String>,
    payload: Result<Json<RuleDraftRequest>, JsonRejection>,
) -> Result<Json<DetectionRuleResponse>, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    let definition = parse_rule_definition(&payload.definition)?;
    let rule = state
        .admin_rules
        .save_draft(
            &rule_slug,
            definition,
            principal.subject(),
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(map_operation)?;
    Ok(Json(rule_response(rule)?))
}

#[utoipa::path(post, path = "/api/v1/admin/detection-rules/{rule_slug}/draft/test", tag = "admin-rules", params(("rule_slug" = String, Path)), request_body = RuleFixtureTestRequest, responses((status = 200, body = RuleFixtureTestResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn test_rule_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(rule_slug): Path<String>,
    payload: Result<Json<RuleFixtureTestRequest>, JsonRejection>,
) -> Result<Json<RuleFixtureTestResponse>, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    if payload.signals.len() > 128 {
        return Err(AdminApiError::Validation);
    }
    let rule = state
        .admin_rules
        .rules()
        .await
        .map_err(map_operation)?
        .into_iter()
        .find(|rule| rule.rule_slug == rule_slug)
        .ok_or(AdminApiError::NotFound)?;
    let definition = rule.draft_definition.ok_or(AdminApiError::Conflict)?;
    let (confidence, threshold) = evaluate_rule_fixture(&definition, &payload.signals)?;
    if confidence >= threshold {
        state
            .admin_rules
            .mark_draft_tested(&rule_slug, principal.subject(), OffsetDateTime::now_utc())
            .await
            .map_err(map_operation)?;
    }
    Ok(Json(RuleFixtureTestResponse {
        passed: confidence >= threshold,
        confidence,
        threshold,
    }))
}

#[utoipa::path(post, path = "/api/v1/admin/detection-rules/{rule_slug}/draft/publish", tag = "admin-rules", params(("rule_slug" = String, Path)), responses((status = 201, body = DetectionRuleVersionResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn publish_rule_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(rule_slug): Path<String>,
) -> Result<(axum::http::StatusCode, Json<DetectionRuleVersionResponse>), AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let version = state
        .admin_rules
        .publish_draft(&rule_slug, principal.subject(), OffsetDateTime::now_utc())
        .await
        .map_err(map_operation)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(rule_version_response(version)?),
    ))
}

#[utoipa::path(patch, path = "/api/v1/admin/detection-rules/{rule_slug}/active-version", tag = "admin-rules", params(("rule_slug" = String, Path)), request_body = ActiveRuleVersionRequest, responses((status = 204), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn set_active_rule_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(rule_slug): Path<String>,
    payload: Result<Json<ActiveRuleVersionRequest>, JsonRejection>,
) -> Result<axum::http::StatusCode, AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    state
        .admin_rules
        .set_active_version(
            &rule_slug,
            payload.version,
            principal.subject(),
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(map_operation)?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/v1/admin/detection-rules/{rule_slug}/versions/{rule_version}/reprocessing", tag = "admin-rules", params(("rule_slug" = String, Path), ("rule_version" = u16, Path)), request_body = StartReprocessingRequest, responses((status = 201, body = ReprocessingRunResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 404, body = ErrorEnvelope), (status = 409, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn start_reprocessing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((rule_slug, rule_version)): Path<(String, u16)>,
    payload: Result<Json<StartReprocessingRequest>, JsonRejection>,
) -> Result<(axum::http::StatusCode, Json<ReprocessingRunResponse>), AdminApiError> {
    let principal = authorize(&*state.admin_auth, &headers, AdminPermission::Operate).await?;
    let Json(payload) = payload.map_err(|_| AdminApiError::Validation)?;
    let run = state
        .admin_reprocessing
        .start_reprocessing(
            &rule_slug,
            rule_version,
            &payload.idempotency_key,
            principal.subject(),
            OffsetDateTime::now_utc(),
        )
        .await
        .map_err(map_operation)?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(reprocessing_run_response(run)?),
    ))
}

#[utoipa::path(get, path = "/api/v1/admin/detection-reprocessing-runs", tag = "admin-rules", params(ReprocessingRunQuery), responses((status = 200, body = ReprocessingRunListResponse), (status = 401, body = ErrorEnvelope), (status = 403, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)), security(("oidc_bearer" = [])))]
pub async fn reprocessing_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ReprocessingRunQuery>,
) -> Result<Json<ReprocessingRunListResponse>, AdminApiError> {
    let _ = authorize(&*state.admin_auth, &headers, AdminPermission::View).await?;
    let runs = state
        .admin_reprocessing
        .reprocessing_runs(query.rule_slug.as_deref(), bounded_run_limit(query.limit)?)
        .await
        .map_err(map_operation)?;
    Ok(Json(ReprocessingRunListResponse {
        runs: runs
            .into_iter()
            .map(reprocessing_run_response)
            .collect::<Result<_, _>>()?,
    }))
}

fn crawl_attempt_response(
    attempt: techatlas_models::AdminCrawlAttempt,
) -> Result<CrawlAttemptResponse, AdminApiError> {
    Ok(CrawlAttemptResponse {
        job_id: attempt.job_id,
        canonical_domain: attempt.canonical_domain,
        attempt_number: attempt.attempt_number,
        status: attempt.status,
        queued_at: attempt
            .queued_at
            .format(&Rfc3339)
            .map_err(|_| AdminApiError::Unavailable)?,
        finished_at: attempt
            .finished_at
            .map(|time| time.format(&Rfc3339))
            .transpose()
            .map_err(|_| AdminApiError::Unavailable)?,
        failure_code: attempt.failure_code,
        failure_summary: attempt.failure_summary,
        retry_state: attempt.retry_state,
        retry_eligible: attempt.retry_eligible,
    })
}

fn rule_response(
    rule: techatlas_models::AdminDetectionRule,
) -> Result<DetectionRuleResponse, AdminApiError> {
    Ok(DetectionRuleResponse {
        rule_slug: rule.rule_slug,
        technology_slug: rule.technology_slug,
        technology_name: rule.technology_name,
        active_version: rule.active_version,
        draft_definition: rule
            .draft_definition
            .map(|definition| serde_json::to_string_pretty(&definition))
            .transpose()
            .map_err(|_| AdminApiError::Unavailable)?,
        draft_updated_at: rule
            .draft_updated_at
            .map(|time| time.format(&Rfc3339))
            .transpose()
            .map_err(|_| AdminApiError::Unavailable)?,
        versions: rule
            .versions
            .into_iter()
            .map(rule_version_response)
            .collect::<Result<_, _>>()?,
    })
}

fn rule_version_response(
    version: techatlas_models::AdminDetectionRuleVersion,
) -> Result<DetectionRuleVersionResponse, AdminApiError> {
    Ok(DetectionRuleVersionResponse {
        version: version.version,
        definition: serde_json::to_string_pretty(&version.definition)
            .map_err(|_| AdminApiError::Unavailable)?,
        published_at: version
            .published_at
            .format(&Rfc3339)
            .map_err(|_| AdminApiError::Unavailable)?,
        is_active: version.is_active,
    })
}

fn reprocessing_run_response(
    run: techatlas_models::AdminReprocessingRun,
) -> Result<ReprocessingRunResponse, AdminApiError> {
    Ok(ReprocessingRunResponse {
        id: run.id,
        rule_slug: run.rule_slug,
        rule_version: run.rule_version,
        status: run.status,
        total_snapshot_count: run.total_snapshot_count,
        succeeded_snapshot_count: run.succeeded_snapshot_count,
        failed_snapshot_count: run.failed_snapshot_count,
        requested_at: run
            .requested_at
            .format(&Rfc3339)
            .map_err(|_| AdminApiError::Unavailable)?,
        started_at: run
            .started_at
            .map(|value| value.format(&Rfc3339))
            .transpose()
            .map_err(|_| AdminApiError::Unavailable)?,
        finished_at: run
            .finished_at
            .map(|value| value.format(&Rfc3339))
            .transpose()
            .map_err(|_| AdminApiError::Unavailable)?,
    })
}

fn bounded_run_limit(limit: Option<usize>) -> Result<usize, AdminApiError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if !(1..=MAX_PAGE_SIZE).contains(&limit) {
        return Err(AdminApiError::Validation);
    }
    Ok(limit)
}

fn parse_rule_definition(value: &str) -> Result<Value, AdminApiError> {
    if value.len() > 32_768 {
        return Err(AdminApiError::Validation);
    }
    let definition: Value = serde_json::from_str(value).map_err(|_| AdminApiError::Validation)?;
    validate_rule_definition(&definition)?;
    Ok(definition)
}

fn validate_rule_definition(definition: &Value) -> Result<(), AdminApiError> {
    let object = definition.as_object().ok_or(AdminApiError::Validation)?;
    let threshold = object
        .get("threshold")
        .and_then(Value::as_u64)
        .filter(|value| (1..=100).contains(value))
        .ok_or(AdminApiError::Validation)?;
    let signals = object
        .get("signals")
        .and_then(Value::as_array)
        .filter(|signals| !signals.is_empty() && signals.len() <= 64)
        .ok_or(AdminApiError::Validation)?;
    let _ = threshold;
    for signal in signals {
        let signal = signal.as_object().ok_or(AdminApiError::Validation)?;
        let source = signal
            .get("source")
            .and_then(Value::as_str)
            .filter(|source| matches!(*source, "html" | "header" | "script" | "dns" | "tls"))
            .ok_or(AdminApiError::Validation)?;
        let key = signal
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty() && key.len() <= 128)
            .ok_or(AdminApiError::Validation)?;
        let matcher = signal
            .get("match")
            .and_then(Value::as_str)
            .filter(|matcher| {
                matches!(
                    *matcher,
                    "equals_ignore_case" | "contains_ignore_case" | "present"
                )
            })
            .ok_or(AdminApiError::Validation)?;
        let weight = signal
            .get("weight")
            .and_then(Value::as_u64)
            .filter(|weight| (1..=100).contains(weight))
            .ok_or(AdminApiError::Validation)?;
        if matcher != "present"
            && signal
                .get("value")
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty() && value.len() <= 2_048)
                .is_none()
        {
            return Err(AdminApiError::Validation);
        }
        let _ = (source, key, weight);
    }
    if let Some(version_evidence) = object.get("version_evidence") {
        let version_evidence = version_evidence
            .as_object()
            .ok_or(AdminApiError::Validation)?;
        version_evidence
            .get("source")
            .and_then(Value::as_str)
            .filter(|source| matches!(*source, "html" | "header" | "script" | "dns" | "tls"))
            .ok_or(AdminApiError::Validation)?;
        version_evidence
            .get("key")
            .and_then(Value::as_str)
            .filter(|key| !key.trim().is_empty() && key.len() <= 128)
            .ok_or(AdminApiError::Validation)?;
        let pattern = version_evidence
            .get("pattern")
            .and_then(Value::as_str)
            .filter(|pattern| !pattern.is_empty() && pattern.len() <= 256)
            .ok_or(AdminApiError::Validation)?;
        let regex = regex::Regex::new(pattern).map_err(|_| AdminApiError::Validation)?;
        if regex.captures_len() != 2 || version_evidence.len() != 3 {
            return Err(AdminApiError::Validation);
        }
    }
    Ok(())
}

fn evaluate_rule_fixture(
    definition: &Value,
    fixture: &[RuleFixtureSignal],
) -> Result<(u8, u8), AdminApiError> {
    validate_rule_definition(definition)?;
    let object = definition.as_object().ok_or(AdminApiError::Validation)?;
    let threshold = u8::try_from(
        object
            .get("threshold")
            .and_then(Value::as_u64)
            .ok_or(AdminApiError::Validation)?,
    )
    .map_err(|_| AdminApiError::Validation)?;
    let signals = object
        .get("signals")
        .and_then(Value::as_array)
        .ok_or(AdminApiError::Validation)?;
    let mut confidence = 0_u8;
    for signal in signals {
        let signal = signal.as_object().ok_or(AdminApiError::Validation)?;
        let source = signal
            .get("source")
            .and_then(Value::as_str)
            .ok_or(AdminApiError::Validation)?;
        let key = signal
            .get("key")
            .and_then(Value::as_str)
            .ok_or(AdminApiError::Validation)?;
        let matcher = signal
            .get("match")
            .and_then(Value::as_str)
            .ok_or(AdminApiError::Validation)?;
        let expected = signal.get("value").and_then(Value::as_str);
        let matched = fixture.iter().any(|item| {
            item.source == source
                && item.key.eq_ignore_ascii_case(key)
                && match matcher {
                    "present" => !item.value.is_empty(),
                    "equals_ignore_case" => {
                        expected.is_some_and(|value| item.value.eq_ignore_ascii_case(value))
                    }
                    "contains_ignore_case" => expected.is_some_and(|value| {
                        item.value
                            .to_ascii_lowercase()
                            .contains(&value.to_ascii_lowercase())
                    }),
                    _ => false,
                }
        });
        if matched {
            let weight = u8::try_from(
                signal
                    .get("weight")
                    .and_then(Value::as_u64)
                    .ok_or(AdminApiError::Validation)?,
            )
            .map_err(|_| AdminApiError::Validation)?;
            confidence = confidence.saturating_add(weight).min(100);
        }
    }
    Ok((confidence, threshold))
}

fn map_operation(error: AdminOperationError) -> AdminApiError {
    match error {
        AdminOperationError::NotFound => AdminApiError::NotFound,
        AdminOperationError::Validation => AdminApiError::Validation,
        AdminOperationError::Conflict => AdminApiError::Conflict,
        AdminOperationError::Unavailable => AdminApiError::Unavailable,
    }
}

fn parse_audit_cursor(value: &str) -> Result<AdminAuditCursor, AdminApiError> {
    let (occurred_at, id) = value.rsplit_once(',').ok_or(AdminApiError::Validation)?;
    let occurred_at =
        OffsetDateTime::parse(occurred_at, &Rfc3339).map_err(|_| AdminApiError::Validation)?;
    AdminAuditCursor::new(occurred_at, id.to_owned()).map_err(map_operation)
}

fn format_audit_cursor(
    cursor: techatlas_models::AdminAuditCursor,
) -> Result<String, AdminApiError> {
    let occurred_at = cursor
        .occurred_at()
        .format(&Rfc3339)
        .map_err(|_| AdminApiError::Unavailable)?;
    Ok(format!("{occurred_at},{}", cursor.id()))
}
