use crate::{
    admin_auth::TestAdminAuthorizer,
    app::{
        AppState, UnavailableAdminOperations, UnavailablePublicReads, UnavailablePublicRefreshes,
        UnavailablePublicSearch, router,
    },
};
use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use std::sync::Arc;
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_models::{
    CanonicalDomain, Domain, DomainRepository, DomainRepositoryError, DomainService, NewDomain,
    PublicDomainProfile, PublicDomainSummary, PublicEvidence, PublicReadError,
    PublicReadOperations, PublicRefreshError, PublicRefreshOperations, PublicRefreshRequest,
    PublicRelatedTechnology, PublicTechnology, PublicTechnologyCategory,
    PublicTechnologyDomainPage, PublicTechnologyHistoryPoint, PublicTechnologyLibraryItem,
    PublicTechnologyLibraryPage, PublicTechnologyProfile, PublicTechnologyTrend,
};
use techatlas_search::{
    DomainSearch, DomainSearchFacets, DomainSearchHit, DomainSearchPage, DomainSearchQuery,
    SearchIndexError,
};
use techatlas_telemetry::TelemetryMetrics;
use time::OffsetDateTime;
use tower::ServiceExt;

struct StaticProbe(DependencyStatus);

struct EmptyDomainRepository;

struct StaticPublicSearch;
struct StaticProfileReads;
struct AcceptedRefresh;
struct RateLimitedRefresh;
struct DisabledRefresh;

#[async_trait]
impl PublicRefreshOperations for AcceptedRefresh {
    async fn request_refresh(
        &self,
        domain: &CanonicalDomain,
        requested_at: OffsetDateTime,
        cooldown: time::Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError> {
        if domain.as_str() != "profile.test" {
            return Err(PublicRefreshError::NotFound);
        }
        Ok(PublicRefreshRequest {
            accepted_at: requested_at,
            next_allowed_at: requested_at
                .checked_add(cooldown)
                .ok_or(PublicRefreshError::Unavailable)?,
        })
    }
}

#[async_trait]
impl PublicRefreshOperations for RateLimitedRefresh {
    async fn request_refresh(
        &self,
        _: &CanonicalDomain,
        _: OffsetDateTime,
        _: time::Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError> {
        Err(PublicRefreshError::RateLimited {
            retry_after_seconds: 86_400,
        })
    }
}

#[async_trait]
impl PublicRefreshOperations for DisabledRefresh {
    async fn request_refresh(
        &self,
        _: &CanonicalDomain,
        _: OffsetDateTime,
        _: time::Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError> {
        Err(PublicRefreshError::Disabled)
    }
}

#[async_trait]
impl DomainSearch for StaticPublicSearch {
    async fn search(&self, _: DomainSearchQuery) -> Result<DomainSearchPage, SearchIndexError> {
        Ok(DomainSearchPage {
            hits: vec![DomainSearchHit {
                canonical_domain: "example.test".to_owned(),
                technology_slugs: vec!["react".to_owned()],
                category_slugs: vec!["frontend-framework".to_owned()],
                country_code: Some("NG".to_owned()),
                last_crawled_at: Some(OffsetDateTime::UNIX_EPOCH),
                updated_at: OffsetDateTime::UNIX_EPOCH,
            }],
            estimated_total_hits: 1,
            facets: DomainSearchFacets {
                technology: [("react".to_owned(), 1)].into_iter().collect(),
                category: [("frontend-framework".to_owned(), 1)].into_iter().collect(),
                country: [("NG".to_owned(), 1)].into_iter().collect(),
            },
        })
    }
}

#[async_trait]
impl PublicReadOperations for StaticProfileReads {
    async fn domain_profile(
        &self,
        domain: &CanonicalDomain,
    ) -> Result<Option<PublicDomainProfile>, PublicReadError> {
        if domain.as_str() != "profile.test" {
            return Ok(None);
        }
        Ok(Some(PublicDomainProfile {
            canonical_domain: "profile.test".to_owned(),
            first_indexed_at: OffsetDateTime::UNIX_EPOCH,
            last_crawled_at: None,
            country_code: None,
            technologies: vec![PublicTechnology {
                slug: "react".to_owned(),
                display_name: "React".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                confidence: 95,
                method: "header".to_owned(),
                rule_version: 3,
                first_observed_at: OffsetDateTime::UNIX_EPOCH,
                last_observed_at: OffsetDateTime::UNIX_EPOCH,
                evidence: vec![PublicEvidence {
                    source: "header".to_owned(),
                    key: "x-powered-by".to_owned(),
                    value: "React".to_owned(),
                }],
            }],
        }))
    }

    async fn domain_changes(
        &self,
        _: &CanonicalDomain,
        _: Option<OffsetDateTime>,
        _: usize,
    ) -> Result<techatlas_models::PublicPage<techatlas_models::PublicChange>, PublicReadError> {
        Err(PublicReadError::Unavailable)
    }

    async fn domain_crawls(
        &self,
        _: &CanonicalDomain,
        _: Option<OffsetDateTime>,
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
        _: Option<PublicTechnologyTrend>,
        _: Option<&str>,
        _: usize,
        _: OffsetDateTime,
    ) -> Result<PublicTechnologyLibraryPage, PublicReadError> {
        Ok(PublicTechnologyLibraryPage {
            items: vec![PublicTechnologyLibraryItem {
                slug: "react".to_owned(),
                display_name: "React".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                adoption_count: 12,
                net_change: 3,
                trend: PublicTechnologyTrend::Growing,
            }],
            categories: vec![PublicTechnologyCategory {
                slug: "frontend-framework".to_owned(),
                display_name: "Frontend framework".to_owned(),
            }],
            next_cursor: None,
        })
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
        technology: &str,
        _: Option<&str>,
        _: usize,
        _: OffsetDateTime,
    ) -> Result<Option<techatlas_models::PublicTechnologyProfile>, PublicReadError> {
        if technology != "react" {
            return Ok(None);
        }
        Ok(Some(PublicTechnologyProfile {
            slug: "react".to_owned(),
            display_name: "React".to_owned(),
            category_slug: "frontend-framework".to_owned(),
            category_name: "Frontend framework".to_owned(),
            adoption_count: 12,
            net_change: 3,
            trend: PublicTechnologyTrend::Growing,
            history: vec![PublicTechnologyHistoryPoint {
                day: "2026-08-04".to_owned(),
                net_change: 3,
            }],
            related_technologies: vec![PublicRelatedTechnology {
                slug: "nextjs".to_owned(),
                display_name: "Next.js".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                shared_domain_count: 7,
            }],
            domains: PublicTechnologyDomainPage {
                items: vec![PublicDomainSummary {
                    canonical_domain: "profile.test".to_owned(),
                    last_crawled_at: Some(OffsetDateTime::UNIX_EPOCH),
                }],
                next_cursor: None,
            },
        }))
    }

    async fn comparison(
        &self,
        domains: &[CanonicalDomain],
        _: OffsetDateTime,
    ) -> Result<Vec<techatlas_models::PublicComparisonCell>, PublicReadError> {
        Ok(domains
            .iter()
            .flat_map(|domain| {
                [
                    techatlas_models::PublicComparisonCell {
                        category_slug: "frontend-framework".to_owned(),
                        canonical_domain: domain.as_str().to_owned(),
                        technology_slug: Some("react".to_owned()),
                        state: "current".to_owned(),
                        last_observed_at: Some(OffsetDateTime::UNIX_EPOCH),
                    },
                    techatlas_models::PublicComparisonCell {
                        category_slug: "cms".to_owned(),
                        canonical_domain: domain.as_str().to_owned(),
                        technology_slug: None,
                        state: "unknown".to_owned(),
                        last_observed_at: None,
                    },
                ]
            })
            .collect())
    }

    async fn analytics_overview(
        &self,
    ) -> Result<techatlas_models::PublicAnalyticsOverview, PublicReadError> {
        Ok(techatlas_models::PublicAnalyticsOverview {
            domain_count: 42,
            technology_count: 9,
            current_detection_count: 87,
            country_count: 3,
        })
    }

    async fn adoption(&self) -> Result<Vec<techatlas_models::PublicAdoption>, PublicReadError> {
        Ok(vec![techatlas_models::PublicAdoption {
            slug: "react".to_owned(),
            count: 21,
        }])
    }

    async fn adoption_history(
        &self,
        _: time::Date,
        _: Option<&str>,
    ) -> Result<techatlas_models::PublicAdoptionHistory, PublicReadError> {
        Ok(techatlas_models::PublicAdoptionHistory {
            status: techatlas_models::PublicAdoptionHistoryStatus::Ready,
            available_from: Some("2026-08-03".to_owned()),
            available_to: Some("2026-08-04".to_owned()),
            series: vec![techatlas_models::PublicAdoptionSeries {
                slug: "react".to_owned(),
                display_name: "React".to_owned(),
                points: vec![
                    techatlas_models::PublicAdoptionPoint {
                        day: "2026-08-03".to_owned(),
                        count: 20,
                    },
                    techatlas_models::PublicAdoptionPoint {
                        day: "2026-08-04".to_owned(),
                        count: 21,
                    },
                ],
            }],
        })
    }

    async fn analytics_rankings(
        &self,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsRankings, PublicReadError> {
        Ok(techatlas_models::PublicAnalyticsRankings {
            technologies: vec![techatlas_models::PublicAnalyticsRank {
                slug: "react".to_owned(),
                display_name: "React".to_owned(),
                count: 21,
            }],
            categories: vec![techatlas_models::PublicAnalyticsRank {
                slug: "frontend-framework".to_owned(),
                display_name: "Frontend framework".to_owned(),
                count: 24,
            }],
            countries: vec![techatlas_models::PublicCountryRank {
                country_code: "NG".to_owned(),
                count: 14,
            }],
            providers: vec![techatlas_models::PublicAnalyticsRank {
                slug: "vercel".to_owned(),
                display_name: "Vercel".to_owned(),
                count: 18,
            }],
        })
    }

    async fn analytics_movers(
        &self,
        _: OffsetDateTime,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsMovers, PublicReadError> {
        Ok(techatlas_models::PublicAnalyticsMovers {
            growing_technologies: vec![techatlas_models::PublicAnalyticsTechnologyMover {
                slug: "react".to_owned(),
                display_name: "React".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                adoption_count: 21,
                net_change: 4,
            }],
            declining_technologies: vec![techatlas_models::PublicAnalyticsTechnologyMover {
                slug: "jquery".to_owned(),
                display_name: "jQuery".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                adoption_count: 7,
                net_change: -2,
            }],
            growing_providers: vec![techatlas_models::PublicAnalyticsProviderMover {
                slug: "vercel".to_owned(),
                display_name: "Vercel".to_owned(),
                adoption_count: 18,
                net_change: 3,
            }],
            declining_providers: vec![],
        })
    }

    async fn change_trends(
        &self,
        _: OffsetDateTime,
    ) -> Result<Vec<techatlas_models::PublicChangeTrend>, PublicReadError> {
        Ok(vec![techatlas_models::PublicChangeTrend {
            day: "2026-08-04".to_owned(),
            added: 4,
            removed: 1,
            migrated: 2,
        }])
    }

    async fn analytics_discovery(
        &self,
        _: OffsetDateTime,
        _: usize,
    ) -> Result<techatlas_models::PublicAnalyticsDiscovery, PublicReadError> {
        Ok(techatlas_models::PublicAnalyticsDiscovery {
            large_migrations: vec![],
            newest_domains: vec![],
            frequently_crawled_domains: vec![],
        })
    }

    async fn provider_profile(
        &self,
        provider: &str,
        _: Option<&str>,
        _: usize,
        _: OffsetDateTime,
    ) -> Result<Option<techatlas_models::PublicProviderProfile>, PublicReadError> {
        if provider != "vercel" {
            return Ok(None);
        }
        Ok(Some(techatlas_models::PublicProviderProfile {
            slug: "vercel".to_owned(),
            display_name: "Vercel".to_owned(),
            adoption_count: 18,
            net_change: 3,
            trend: PublicTechnologyTrend::Growing,
            technologies: vec![PublicTechnologyLibraryItem {
                slug: "nextjs".to_owned(),
                display_name: "Next.js".to_owned(),
                category_slug: "frontend-framework".to_owned(),
                category_name: "Frontend framework".to_owned(),
                adoption_count: 18,
                net_change: 3,
                trend: PublicTechnologyTrend::Growing,
            }],
            domains: techatlas_models::PublicProviderDomainPage {
                items: vec![PublicDomainSummary {
                    canonical_domain: "profile.test".to_owned(),
                    last_crawled_at: Some(OffsetDateTime::UNIX_EPOCH),
                }],
                next_cursor: None,
            },
        }))
    }
}

#[async_trait]
impl DomainRepository for EmptyDomainRepository {
    async fn create(&self, _: NewDomain) -> Result<Domain, DomainRepositoryError> {
        Err(DomainRepositoryError::Unavailable)
    }

    async fn create_with_audit(
        &self,
        _: NewDomain,
        _: &str,
    ) -> Result<Domain, DomainRepositoryError> {
        Err(DomainRepositoryError::Unavailable)
    }

    async fn find_active_by_canonical(
        &self,
        _: &CanonicalDomain,
    ) -> Result<Option<Domain>, DomainRepositoryError> {
        Ok(None)
    }

    async fn list_active(
        &self,
        _: Option<&CanonicalDomain>,
        _: usize,
    ) -> Result<Vec<Domain>, DomainRepositoryError> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl DependencyProbe for StaticProbe {
    async fn check(&self) -> DependencyStatus {
        self.0
    }
}

fn state(
    postgres: DependencyStatus,
    redis: DependencyStatus,
    meilisearch: DependencyStatus,
) -> AppState {
    AppState {
        metrics: Arc::new(TelemetryMetrics::new("test_api").expect("metrics should initialise")),
        postgres: Arc::new(StaticProbe(postgres)),
        redis: Arc::new(StaticProbe(redis)),
        meilisearch: Arc::new(StaticProbe(meilisearch)),
        admin_domains: Arc::new(DomainService::with_default_policy(EmptyDomainRepository)),
        admin_auth: Arc::new(TestAdminAuthorizer),
        admin_imports: Arc::new(UnavailableAdminOperations),
        admin_policies: Arc::new(UnavailableAdminOperations),
        admin_audits: Arc::new(UnavailableAdminOperations),
        admin_operations: Arc::new(UnavailableAdminOperations),
        admin_queue: Arc::new(UnavailableAdminOperations),
        admin_scheduler: Arc::new(UnavailableAdminOperations),
        admin_rules: Arc::new(UnavailableAdminOperations),
        admin_reprocessing: Arc::new(UnavailableAdminOperations),
        public_reads: Arc::new(UnavailablePublicReads),
        public_refreshes: Arc::new(UnavailablePublicRefreshes),
        public_search: Arc::new(UnavailablePublicSearch),
        public_stale_after_days: 30,
        public_refresh_cooldown_hours: 24,
    }
}

#[tokio::test]
async fn liveness_is_process_only() {
    let response = router(state(
        DependencyStatus::Unavailable,
        DependencyStatus::Unavailable,
        DependencyStatus::Unavailable,
    ))
    .oneshot(
        Request::get("/healthz")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn metrics_are_scrapeable_and_readiness_updates_dependency_gauges() {
    let app = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ));
    let ready = app
        .clone()
        .oneshot(
            Request::get("/readyz")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");
    assert_eq!(ready.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::get("/metrics")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("metrics body should read");
    let body = String::from_utf8(body.to_vec()).expect("metrics body should be UTF-8");
    assert!(body.contains("techatlas_dependency_ready"));
    assert!(body.contains("dependency=\"postgres\""));
}

#[tokio::test]
async fn readiness_requires_every_dependency() {
    let ready_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/readyz")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(ready_response.status(), StatusCode::OK);

    let unavailable_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Unavailable,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/readyz")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(
        unavailable_response.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn public_search_validates_pagination_and_public_reads_do_not_require_admin_auth() {
    let validation_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/api/v1/public/search/domains?limit=101")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(
        validation_response.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let unavailable_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/api/v1/public/domains/example.com")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(
        unavailable_response.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[tokio::test]
async fn public_search_returns_a_typed_page_without_admin_authentication() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_search = Arc::new(StaticPublicSearch);
    let response = router(app_state)
        .oneshot(
            Request::get(
                "/api/v1/public/search/domains?q=example&technology=react&technology=nextjs&sort=updated_desc",
            )
            .body(Body::empty())
            .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains(r#""results":[{"canonical_domain":"example.test""#));
    assert!(body.contains(r#""technology_slugs":["react"]"#));
    assert!(body.contains(r#""estimated_total_hits":1"#));
    assert!(body.contains(r#""facets":{"technology":{"react":1},"category":{"frontend-framework":1},"country":{"NG":1}}"#));
}

#[tokio::test]
async fn public_domain_profile_returns_typed_current_evidence_without_admin_authentication() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_reads = Arc::new(StaticProfileReads);
    let response = router(app_state)
        .oneshot(
            Request::get("/api/v1/public/domains/profile.test")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains(r#""canonical_domain":"profile.test""#));
    assert!(body.contains(r#""confidence":95"#));
    assert!(
        body.contains(r#""evidence":[{"source":"header","key":"x-powered-by","value":"React"}]"#)
    );
}

#[tokio::test]
async fn public_technology_library_returns_typed_trend_data_and_validates_filters() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_reads = Arc::new(StaticProfileReads);
    let response = router(app_state)
        .oneshot(
            Request::get("/api/v1/public/technologies?category=frontend-framework&trend=growing")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains(r#""net_change":3"#));
    assert!(body.contains(r#""trend":"growing""#));
    assert!(body.contains(r#""categories":[{"slug":"frontend-framework""#));

    let invalid_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/api/v1/public/technologies?trend=unknown")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(invalid_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn public_technology_profile_returns_adoption_history_related_technologies_and_domains() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_reads = Arc::new(StaticProfileReads);
    let response = router(app_state)
        .oneshot(
            Request::get("/api/v1/public/technologies/react/profile")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains(r#""adoption_count":12"#));
    assert!(body.contains(r#""history":[{"day":"2026-08-04","net_change":3}]"#));
    assert!(body.contains(r#""related_technologies":[{"slug":"nextjs""#));
    assert!(body.contains(r#""domains":{"items":[{"canonical_domain":"profile.test""#));
}

#[tokio::test]
async fn public_analytics_history_and_discovery_are_typed_and_validate_the_window() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_reads = Arc::new(StaticProfileReads);
    let history_response = router(app_state.clone())
        .oneshot(
            Request::get(
                "/api/v1/public/analytics/adoption-history?since_days=90&technology=react",
            )
            .body(Body::empty())
            .expect("request should build"),
        )
        .await
        .expect("route should respond");
    assert_eq!(history_response.status(), StatusCode::OK);
    let history_body = to_bytes(history_response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let history_body = String::from_utf8(history_body.to_vec()).expect("response should be UTF-8");
    assert!(history_body.contains(r#""status":"ready""#));
    assert!(history_body.contains(r#""points":[{"day":"2026-08-03","count":20}"#));

    let discovery_response = router(app_state)
        .oneshot(
            Request::get("/api/v1/public/analytics/discovery?since_days=365")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");
    assert_eq!(discovery_response.status(), StatusCode::OK);

    let invalid_response = router(state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    ))
    .oneshot(
        Request::get("/api/v1/public/analytics/adoption-history?since_days=366")
            .body(Body::empty())
            .expect("request should build"),
    )
    .await
    .expect("route should respond");
    assert_eq!(invalid_response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn public_comparison_returns_a_typed_normalized_category_matrix() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_reads = Arc::new(StaticProfileReads);
    let response = router(app_state)
        .oneshot(
            Request::get("/api/v1/public/compare?domain=Profile.test&domain=EXAMPLE.test")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains(r#""domains":["profile.test","example.test"]"#));
    assert!(body.contains(r#""category_slug":"frontend-framework""#));
    assert!(body.contains(r#""state":"unknown""#));
}

#[tokio::test]
async fn public_refresh_request_is_accepted_without_direct_queue_access() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_refreshes = Arc::new(AcceptedRefresh);
    let response = router(app_state)
        .oneshot(
            Request::post("/api/v1/public/domains/profile.test")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should read");
    let body = String::from_utf8(body.to_vec()).expect("response should be UTF-8");
    assert!(body.contains("accepted_at"));
    assert!(body.contains("next_allowed_at"));
}

#[tokio::test]
async fn public_refresh_request_reports_cooldown_with_retry_after() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_refreshes = Arc::new(RateLimitedRefresh);
    let response = router(app_state)
        .oneshot(
            Request::post("/api/v1/public/domains/profile.test")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok()),
        Some("86400")
    );
}

#[tokio::test]
async fn public_refresh_request_does_not_accept_disabled_crawl_policies() {
    let mut app_state = state(
        DependencyStatus::Ready,
        DependencyStatus::Ready,
        DependencyStatus::Ready,
    );
    app_state.public_refreshes = Arc::new(DisabledRefresh);
    let response = router(app_state)
        .oneshot(
            Request::post("/api/v1/public/domains/profile.test")
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("route should respond");

    assert_eq!(response.status(), StatusCode::CONFLICT);
}
