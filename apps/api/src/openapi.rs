use crate::admin_domains::{
    CreateDomainRequest, DomainListResponse, DomainResponse, ErrorBody, ErrorEnvelope,
};
use crate::admin_operations::{
    ActiveRuleVersionRequest, AuditEventPageResponse, AuditEventResponse,
    CountryEnrichmentRecrawlResponse, CrawlAttemptPageResponse, CrawlAttemptResponse,
    CrawlRequestResponse, CsvImportRequest, CsvImportResponse, DependencyStatusResponse,
    DetectionRuleListResponse, DetectionRuleResponse, DetectionRuleVersionResponse,
    ImportBatchListResponse, ImportBatchRecrawlResponse, ImportBatchResponse,
    OperationsActivityResponse, OperationsAlertResponse, OperationsOverviewResponse,
    PolicyResponse, QueueOverviewResponse, ReprocessingRunListResponse, ReprocessingRunResponse,
    RuleDraftRequest, RuleFixtureSignal, RuleFixtureTestRequest, RuleFixtureTestResponse,
    StartReprocessingRequest, ThroughputPointResponse, UpdatePolicyRequest, WorkerOverviewResponse,
};
use techatlas_models::{
    AdminCrawlRetryState, PublicAdoption, PublicAdoptionHistory, PublicAdoptionHistoryStatus,
    PublicAdoptionPoint, PublicAdoptionSeries, PublicAnalyticsDiscovery, PublicAnalyticsDomain,
    PublicAnalyticsMovers, PublicAnalyticsOverview, PublicAnalyticsProviderMover,
    PublicAnalyticsRank, PublicAnalyticsRankings, PublicAnalyticsTechnologyMover, PublicChange,
    PublicChangePage, PublicChangeTrend, PublicComparisonCell, PublicComparisonResponse,
    PublicCountryRank, PublicCrawl, PublicCrawlDetail, PublicCrawlPage, PublicDnsObservation,
    PublicDomainProfile, PublicDomainSearchFacets, PublicDomainSearchHit, PublicDomainSearchPage,
    PublicEvidence, PublicFrequentCrawlDomain, PublicProviderDomainPage, PublicProviderProfile,
    PublicRedirect, PublicRefreshRequest, PublicRelatedTechnology, PublicTechnology,
    PublicTechnologyCategory, PublicTechnologyDomainPage, PublicTechnologyHistoryPoint,
    PublicTechnologyLibraryItem, PublicTechnologyLibraryPage, PublicTechnologyMigration,
    PublicTechnologyProfile, PublicTechnologyTrend, PublicTlsObservation,
};
use utoipa::openapi::{
    OpenApi as OpenApiDocument,
    security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(title = "TechAtlas API", version = "1.0.0"),
    paths(
        crate::admin_domains::create_domain,
        crate::admin_domains::list_domains,
        crate::admin_domains::get_domain,
        crate::admin_operations::import_csv,
        crate::admin_operations::completed_import_batches,
        crate::admin_operations::schedule_import_batch_recrawl,
        crate::admin_operations::update_policy,
        crate::admin_operations::request_domain_crawl,
        crate::admin_operations::audit_events,
        crate::admin_operations::operations_overview,
        crate::admin_operations::crawl_attempts,
        crate::admin_operations::retry_crawl_attempt,
        crate::admin_operations::schedule_country_enrichment_recrawl,
        crate::admin_operations::detection_rules,
        crate::admin_operations::save_rule_draft,
        crate::admin_operations::test_rule_draft,
        crate::admin_operations::publish_rule_draft,
        crate::admin_operations::set_active_rule_version,
        crate::admin_operations::start_reprocessing,
        crate::admin_operations::reprocessing_runs,
        crate::public::search_domains,
        crate::public::domain_profile,
        crate::public::request_refresh,
        crate::public::domain_changes,
        crate::public::domain_crawls,
        crate::public::crawl_detail,
        crate::public::technologies,
        crate::public::technology_domains,
        crate::public::technology_profile,
        crate::public::compare,
        crate::public::analytics_overview,
        crate::public::analytics_adoption,
        crate::public::analytics_adoption_history,
        crate::public::analytics_rankings,
        crate::public::analytics_movers,
        crate::public::analytics_changes,
        crate::public::analytics_discovery,
        crate::public::provider_profile,
    ),
    components(schemas(
        CreateDomainRequest,
        DomainListResponse,
        DomainResponse,
        ErrorBody,
        ErrorEnvelope,
        CsvImportRequest,
        CsvImportResponse,
        ImportBatchResponse,
        ImportBatchListResponse,
        ImportBatchRecrawlResponse,
        UpdatePolicyRequest,
        PolicyResponse,
        CrawlRequestResponse,
        AuditEventResponse,
        AuditEventPageResponse,
        OperationsOverviewResponse,
        DependencyStatusResponse,
        OperationsAlertResponse,
        QueueOverviewResponse,
        ThroughputPointResponse,
        WorkerOverviewResponse,
        OperationsActivityResponse,
        CrawlAttemptResponse,
        CrawlAttemptPageResponse,
        CountryEnrichmentRecrawlResponse,
        AdminCrawlRetryState,
        RuleDraftRequest,
        RuleFixtureSignal,
        RuleFixtureTestRequest,
        RuleFixtureTestResponse,
        ActiveRuleVersionRequest,
        DetectionRuleVersionResponse,
        DetectionRuleResponse,
        DetectionRuleListResponse,
        StartReprocessingRequest,
        ReprocessingRunResponse,
        ReprocessingRunListResponse,
        PublicChange,
        PublicChangePage,
        PublicChangeTrend,
        PublicComparisonCell,
        PublicComparisonResponse,
        PublicCrawl,
        PublicCrawlDetail,
        PublicCrawlPage,
        PublicDnsObservation,
        PublicDomainProfile,
        PublicDomainSearchFacets,
        PublicDomainSearchHit,
        PublicDomainSearchPage,
        PublicEvidence,
        PublicRedirect,
        PublicTlsObservation,
        PublicRefreshRequest,
        PublicAdoption,
        PublicAdoptionHistory,
        PublicAdoptionHistoryStatus,
        PublicAdoptionPoint,
        PublicAdoptionSeries,
        PublicAnalyticsDiscovery,
        PublicAnalyticsDomain,
        PublicAnalyticsMovers,
        PublicAnalyticsOverview,
        PublicAnalyticsProviderMover,
        PublicAnalyticsRank,
        PublicAnalyticsRankings,
        PublicAnalyticsTechnologyMover,
        PublicCountryRank,
        PublicFrequentCrawlDomain,
        PublicProviderDomainPage,
        PublicProviderProfile,
        PublicTechnology,
        PublicTechnologyCategory,
        PublicTechnologyLibraryItem,
        PublicTechnologyLibraryPage,
        PublicTechnologyMigration,
        PublicTechnologyDomainPage,
        PublicTechnologyHistoryPoint,
        PublicTechnologyProfile,
        PublicTechnologyTrend,
        PublicRelatedTechnology,
    )),
    modifiers(&AdminSecurity)
)]
pub struct ApiDoc;

struct AdminSecurity;

impl Modify for AdminSecurity {
    fn modify(&self, openapi: &mut OpenApiDocument) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "oidc_bearer",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("Bearer")
                        .build(),
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_contract_is_current() {
        let generated = ApiDoc::openapi()
            .to_yaml()
            .expect("OpenAPI document should serialize");

        assert_eq!(
            generated,
            include_str!("../../../contracts/openapi/openapi.yaml")
        );
    }
}
