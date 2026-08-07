use crate::{
    admin_auth::TestAdminAuthorizer,
    app::{
        AppState, UnavailableAdminOperations, UnavailablePublicReads, UnavailablePublicRefreshes,
        UnavailablePublicSearch, router,
    },
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_models::{
    AdminActivityKind, AdminCrawlAttempt, AdminImportBatch, AdminImportBatchRecrawl,
    AdminImportOperations, AdminOperationError, AdminOperationsActivity, AdminOperationsOverview,
    AdminOperationsRead, AdminQueueRead, AdminQueueSnapshot, AdminSchedulerOperations,
    AdminThroughputPoint, CanonicalDomain, CsvDomainImport, CsvImportResult, Domain, DomainId,
    DomainRepository, DomainRepositoryError, DomainService, NewDomain,
};
use techatlas_telemetry::TelemetryMetrics;
use time::OffsetDateTime;
use tower::ServiceExt;
use uuid::Uuid;

struct StaticProbe;

struct StaticOperations;

#[async_trait]
impl AdminOperationsRead for StaticOperations {
    async fn overview(
        &self,
        now: OffsetDateTime,
    ) -> Result<AdminOperationsOverview, AdminOperationError> {
        Ok(AdminOperationsOverview {
            domain_count: 2,
            technology_count: 3,
            current_detection_count: 4,
            successful_crawl_count: 5,
            scheduled_crawl_count: 6,
            throughput: vec![AdminThroughputPoint {
                observed_at: now,
                completed_count: 7,
            }],
            workers: vec![],
            activity: vec![AdminOperationsActivity {
                id: "event-1".to_owned(),
                title: "domain created".to_owned(),
                description: "domain operation recorded".to_owned(),
                occurred_at: now,
                kind: AdminActivityKind::System,
            }],
        })
    }
}

#[async_trait]
impl AdminQueueRead for StaticOperations {
    async fn snapshot(&self) -> Result<AdminQueueSnapshot, AdminOperationError> {
        Ok(AdminQueueSnapshot {
            ready_count: 8,
            processing_count: 9,
        })
    }
}

#[async_trait]
impl DependencyProbe for StaticProbe {
    async fn check(&self) -> DependencyStatus {
        DependencyStatus::Ready
    }
}

struct InMemoryDomainRepository {
    state: Arc<InMemoryDomainState>,
}

struct InMemoryDomainState {
    domains: Mutex<Vec<Domain>>,
    audit_subjects: Mutex<Vec<String>>,
    next_id: AtomicU64,
}

struct RecordingScheduler {
    requested: Mutex<Vec<(String, String)>>,
    batch_requests: Mutex<Vec<(String, String)>>,
}

struct StaticImports;

#[async_trait]
impl AdminImportOperations for StaticImports {
    async fn import_csv(
        &self,
        _: &str,
        _: &str,
        _: CsvDomainImport,
    ) -> Result<CsvImportResult, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }

    async fn completed_import_batches(
        &self,
        _: usize,
    ) -> Result<Vec<AdminImportBatch>, AdminOperationError> {
        Ok(vec![AdminImportBatch {
            import_id: "00000000-0000-0000-0000-000000000123".to_owned(),
            source_name: "top domains".to_owned(),
            completed_at: OffsetDateTime::UNIX_EPOCH,
            domain_count: 500,
        }])
    }
}

#[async_trait]
impl AdminSchedulerOperations for RecordingScheduler {
    async fn request_domain_crawl(
        &self,
        domain: &CanonicalDomain,
        actor_subject: &str,
        _: OffsetDateTime,
    ) -> Result<(), AdminOperationError> {
        self.requested
            .lock()
            .expect("test lock should not be poisoned")
            .push((domain.as_str().to_owned(), actor_subject.to_owned()));
        Ok(())
    }

    async fn crawl_attempts(
        &self,
        _: usize,
    ) -> Result<Vec<AdminCrawlAttempt>, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }

    async fn retry_terminal_attempt(
        &self,
        _: &str,
        _: &str,
        _: OffsetDateTime,
    ) -> Result<AdminCrawlAttempt, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }

    async fn schedule_country_enrichment_recrawl(
        &self,
        _: &str,
        _: OffsetDateTime,
    ) -> Result<u64, AdminOperationError> {
        Err(AdminOperationError::Unavailable)
    }

    async fn schedule_import_batch_recrawl(
        &self,
        import_id: &str,
        actor_subject: &str,
        _: OffsetDateTime,
    ) -> Result<AdminImportBatchRecrawl, AdminOperationError> {
        self.batch_requests
            .lock()
            .expect("test lock should not be poisoned")
            .push((import_id.to_owned(), actor_subject.to_owned()));
        Ok(AdminImportBatchRecrawl {
            import_id: import_id.to_owned(),
            source_name: "top domains".to_owned(),
            requested_domain_count: 500,
            scheduled_domain_count: 490,
            skipped_domain_count: 10,
        })
    }
}

impl InMemoryDomainState {
    fn new() -> Self {
        Self {
            domains: Mutex::new(Vec::new()),
            audit_subjects: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
        }
    }
}

impl InMemoryDomainRepository {
    fn insert(&self, domain: NewDomain) -> Result<Domain, DomainRepositoryError> {
        let mut domains = self
            .state
            .domains
            .lock()
            .expect("test lock should not be poisoned");
        if domains
            .iter()
            .any(|existing| existing.canonical_domain() == domain.canonical_domain())
        {
            return Err(DomainRepositoryError::Duplicate);
        }

        let created = Domain::new(
            DomainId::from_uuid(Uuid::from_u128(u128::from(
                self.state.next_id.fetch_add(1, Ordering::Relaxed),
            ))),
            domain.canonical_domain().clone(),
        );
        domains.push(created.clone());
        Ok(created)
    }
}

#[async_trait]
impl DomainRepository for InMemoryDomainRepository {
    async fn create(&self, domain: NewDomain) -> Result<Domain, DomainRepositoryError> {
        self.insert(domain)
    }

    async fn create_with_audit(
        &self,
        domain: NewDomain,
        actor_subject: &str,
    ) -> Result<Domain, DomainRepositoryError> {
        let created = self.insert(domain)?;
        self.state
            .audit_subjects
            .lock()
            .expect("test lock should not be poisoned")
            .push(actor_subject.to_owned());
        Ok(created)
    }

    async fn find_active_by_canonical(
        &self,
        canonical_domain: &CanonicalDomain,
    ) -> Result<Option<Domain>, DomainRepositoryError> {
        Ok(self
            .state
            .domains
            .lock()
            .expect("test lock should not be poisoned")
            .iter()
            .find(|domain| domain.canonical_domain() == canonical_domain)
            .cloned())
    }

    async fn list_active(
        &self,
        after: Option<&CanonicalDomain>,
        limit: usize,
    ) -> Result<Vec<Domain>, DomainRepositoryError> {
        let mut domains = self
            .state
            .domains
            .lock()
            .expect("test lock should not be poisoned")
            .iter()
            .filter(|domain| {
                after.is_none_or(|cursor| domain.canonical_domain().as_str() > cursor.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        domains.sort_by(|left, right| {
            left.canonical_domain()
                .as_str()
                .cmp(right.canonical_domain().as_str())
        });
        domains.truncate(limit);
        Ok(domains)
    }
}

fn app(_requests_per_minute: u32) -> (Router, Arc<InMemoryDomainState>) {
    app_with_components(
        _requests_per_minute,
        Arc::new(UnavailableAdminOperations),
        Arc::new(UnavailableAdminOperations),
    )
}

fn app_with_scheduler(
    _requests_per_minute: u32,
    admin_scheduler: Arc<dyn AdminSchedulerOperations>,
) -> (Router, Arc<InMemoryDomainState>) {
    app_with_components(
        _requests_per_minute,
        admin_scheduler,
        Arc::new(UnavailableAdminOperations),
    )
}

fn app_with_components(
    _requests_per_minute: u32,
    admin_scheduler: Arc<dyn AdminSchedulerOperations>,
    admin_imports: Arc<dyn AdminImportOperations>,
) -> (Router, Arc<InMemoryDomainState>) {
    let repository = Arc::new(InMemoryDomainState::new());
    let state = AppState {
        metrics: Arc::new(TelemetryMetrics::new("test_api").expect("metrics should initialise")),
        postgres: Arc::new(StaticProbe),
        redis: Arc::new(StaticProbe),
        meilisearch: Arc::new(StaticProbe),
        admin_domains: Arc::new(DomainService::with_default_policy(
            InMemoryDomainRepository {
                state: Arc::clone(&repository),
            },
        )),
        admin_auth: Arc::new(TestAdminAuthorizer),
        admin_imports,
        admin_policies: Arc::new(UnavailableAdminOperations),
        admin_audits: Arc::new(UnavailableAdminOperations),
        admin_operations: Arc::new(StaticOperations),
        admin_queue: Arc::new(StaticOperations),
        admin_scheduler,
        admin_rules: Arc::new(UnavailableAdminOperations),
        admin_reprocessing: Arc::new(UnavailableAdminOperations),
        public_reads: Arc::new(UnavailablePublicReads),
        public_refreshes: Arc::new(UnavailablePublicRefreshes),
        public_search: Arc::new(UnavailablePublicSearch),
        public_stale_after_days: 30,
        public_refresh_cooldown_hours: 24,
    };

    (router(state), repository)
}

#[tokio::test]
async fn viewer_can_read_the_typed_operations_overview() {
    let (app, _) = app(60);
    let response = app
        .oneshot(authorized_as(
            Request::get("/api/v1/admin/operations/overview")
                .body(Body::empty())
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(bytes.to_vec()).expect("response should be JSON");
    assert!(body.contains(r#""ready_count":8"#));
    assert!(body.contains(r#""scheduled_count":6"#));
    assert!(body.contains(r#""domain_count":2"#));
    assert!(body.contains(r#""dependencies":[{"name":"postgres","status":"ready"}"#));
    assert!(body.contains(r#""alertable_failures":[]"#));
}

fn authorized(request: Request<Body>) -> Request<Body> {
    authorized_as(request, "operator-token")
}

fn authorized_as(request: Request<Body>, token: &str) -> Request<Body> {
    let (mut parts, body) = request.into_parts();
    parts.headers.insert(
        header::AUTHORIZATION,
        format!("Bearer {token}")
            .parse()
            .expect("valid test header"),
    );
    Request::from_parts(parts, body)
}

#[tokio::test]
async fn authenticated_admin_creation_normalizes_and_audits_the_domain() {
    let (app, repository) = app(60);
    let response = app
        .oneshot(authorized(
            Request::post("/api/v1/admin/domains")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"domain":"HTTPS://Example.COM/path"}"#))
                .expect("request should build"),
        ))
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(
        response.headers().get(header::LOCATION),
        Some(
            &"/api/v1/admin/domains/example.com"
                .parse()
                .expect("valid header")
        )
    );
    assert_eq!(
        repository
            .audit_subjects
            .lock()
            .expect("test lock should not be poisoned")
            .as_slice(),
        ["operator"]
    );
}

#[tokio::test]
async fn read_permission_can_read_but_cannot_mutate_domains() {
    let (app, _) = app(60);
    let read = app
        .clone()
        .oneshot(authorized_as(
            Request::get("/api/v1/admin/domains")
                .body(Body::empty())
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(read.status(), StatusCode::OK);

    let mutation = app
        .oneshot(authorized_as(
            Request::post("/api/v1/admin/domains")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"domain":"viewer.example"}"#))
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(mutation.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn quick_crawl_requires_an_operator_and_normalizes_the_target() {
    let scheduler = Arc::new(RecordingScheduler {
        requested: Mutex::new(Vec::new()),
        batch_requests: Mutex::new(Vec::new()),
    });
    let (app, _) = app_with_scheduler(
        60,
        Arc::clone(&scheduler) as Arc<dyn AdminSchedulerOperations>,
    );

    let accepted = app
        .clone()
        .oneshot(authorized(
            Request::post("/api/v1/admin/domains/Example.COM/crawl")
                .body(Body::empty())
                .expect("request should build"),
        ))
        .await
        .expect("route should respond");
    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    assert_eq!(
        scheduler
            .requested
            .lock()
            .expect("test lock should not be poisoned")
            .as_slice(),
        [("example.com".to_owned(), "operator".to_owned())]
    );

    let forbidden = app
        .oneshot(authorized_as(
            Request::post("/api/v1/admin/domains/example.com/crawl")
                .body(Body::empty())
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admins_can_list_and_recrawl_completed_import_batches() {
    let scheduler = Arc::new(RecordingScheduler {
        requested: Mutex::new(Vec::new()),
        batch_requests: Mutex::new(Vec::new()),
    });
    let (app, _) = app_with_components(
        60,
        Arc::clone(&scheduler) as Arc<dyn AdminSchedulerOperations>,
        Arc::new(StaticImports),
    );

    let list = app
        .clone()
        .oneshot(authorized_as(
            Request::get("/api/v1/admin/imports?limit=10")
                .body(Body::empty())
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(list.status(), StatusCode::OK);
    let list_body = String::from_utf8(
        to_bytes(list.into_body(), usize::MAX)
            .await
            .expect("response body should read")
            .to_vec(),
    )
    .expect("response should be JSON");
    assert!(list_body.contains(r#""source_name":"top domains""#));
    assert!(list_body.contains(r#""domain_count":500"#));

    let recrawl = app
        .clone()
        .oneshot(authorized(
            Request::post("/api/v1/admin/imports/00000000-0000-0000-0000-000000000123/recrawl")
                .body(Body::empty())
                .expect("request should build"),
        ))
        .await
        .expect("route should respond");
    assert_eq!(recrawl.status(), StatusCode::OK);
    assert_eq!(
        scheduler
            .batch_requests
            .lock()
            .expect("test lock should not be poisoned")
            .as_slice(),
        [(
            "00000000-0000-0000-0000-000000000123".to_owned(),
            "operator".to_owned()
        )]
    );

    let forbidden = app
        .oneshot(authorized_as(
            Request::post("/api/v1/admin/imports/00000000-0000-0000-0000-000000000123/recrawl")
                .body(Body::empty())
                .expect("request should build"),
            "viewer-token",
        ))
        .await
        .expect("route should respond");
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_routes_reject_missing_authentication_with_the_stable_error_shape() {
    let (app, _) = app(60);
    let response = app
        .oneshot(
            Request::get("/api/v1/admin/domains")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::WWW_AUTHENTICATE),
        Some(&"Bearer".parse().expect("valid header"))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(bytes.to_vec()).expect("response should be UTF-8 JSON");
    assert!(body.contains(r#""code":"unauthorized""#));
}

#[tokio::test]
async fn domain_list_paginates_in_canonical_order_and_rejects_duplicates() {
    let (app, _) = app(60);
    for domain in ["charlie.example", "alpha.example", "bravo.example"] {
        let response = app
            .clone()
            .oneshot(authorized(
                Request::post("/api/v1/admin/domains")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(r#"{{"domain":"{domain}"}}"#)))
                    .expect("request should build"),
            ))
            .await
            .expect("route should respond");
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    let duplicate = app
        .clone()
        .oneshot(authorized(
            Request::post("/api/v1/admin/domains")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"domain":"alpha.example"}"#))
                .expect("request should build"),
        ))
        .await
        .expect("route should respond");
    assert_eq!(duplicate.status(), StatusCode::CONFLICT);

    let response = app
        .oneshot(authorized(
            Request::get("/api/v1/admin/domains?limit=2")
                .body(Body::empty())
                .expect("request should build"),
        ))
        .await
        .expect("route should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(bytes.to_vec()).expect("response should be UTF-8 JSON");
    assert!(body.contains("alpha.example"));
    assert!(body.contains("bravo.example"));
    assert!(body.contains(r#""next_cursor":"bravo.example""#));
}
