use crate::CanonicalDomain;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use thiserror::Error;
use time::OffsetDateTime;
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicEvidence {
    pub source: String,
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnology {
    pub slug: String,
    pub display_name: String,
    pub category_slug: String,
    pub category_name: String,
    pub confidence: u8,
    pub method: String,
    pub rule_version: u16,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub first_observed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub last_observed_at: OffsetDateTime,
    pub evidence: Vec<PublicEvidence>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDomainProfile {
    pub canonical_domain: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub first_indexed_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_crawled_at: Option<OffsetDateTime>,
    pub country_code: Option<String>,
    pub technologies: Vec<PublicTechnology>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicChange {
    pub kind: String,
    pub category_slug: String,
    pub from_technology_slug: Option<String>,
    pub to_technology_slug: Option<String>,
    pub technology_slug: Option<String>,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub reprocessing_run_id: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub observed_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicCrawl {
    pub id: String,
    pub requested_url: String,
    pub final_url: String,
    pub response_status: u16,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub captured_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicCrawlDetail {
    pub crawl: PublicCrawl,
    pub response_headers: std::collections::BTreeMap<String, String>,
    pub redirect_chain: Vec<PublicRedirect>,
    pub country_code: Option<String>,
    pub dns: PublicDnsObservation,
    pub tls: PublicTlsObservation,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicRedirect {
    pub from_url: String,
    pub to_url: String,
    pub status: u16,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDnsObservation {
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub observed_at: OffsetDateTime,
    pub availability: String,
    pub unavailable_reason: Option<String>,
    pub queried_name: Option<String>,
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTlsObservation {
    pub source: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub observed_at: OffsetDateTime,
    pub availability: String,
    pub unavailable_reason: Option<String>,
    pub validation_status: Option<String>,
    pub protocol: Option<String>,
    pub cipher_suite: Option<String>,
    pub certificate_subject: Option<String>,
    pub certificate_issuer: Option<String>,
    pub subject_alternative_names: Vec<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub certificate_not_before: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub certificate_not_after: Option<OffsetDateTime>,
}

/// A cursor page of immutable technology changes for one public domain.
#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicChangePage {
    pub items: Vec<PublicChange>,
    pub next_cursor: Option<String>,
}

impl From<PublicPage<PublicChange>> for PublicChangePage {
    fn from(value: PublicPage<PublicChange>) -> Self {
        Self {
            items: value.items,
            next_cursor: value.next_cursor,
        }
    }
}

/// A cursor page of immutable crawl snapshots for one public domain.
#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicCrawlPage {
    pub items: Vec<PublicCrawl>,
    pub next_cursor: Option<String>,
}

impl From<PublicPage<PublicCrawl>> for PublicCrawlPage {
    fn from(value: PublicPage<PublicCrawl>) -> Self {
        Self {
            items: value.items,
            next_cursor: value.next_cursor,
        }
    }
}

/// Confirmation that a public refresh request was recorded for scheduler processing.
#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicRefreshRequest {
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub accepted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub next_allowed_at: OffsetDateTime,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicTechnologyTrend {
    Growing,
    Declining,
    Stable,
}

impl PublicTechnologyTrend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Growing => "growing",
            Self::Declining => "declining",
            Self::Stable => "stable",
        }
    }

    pub fn from_net_change(net_change: i64) -> Self {
        match net_change.cmp(&0) {
            std::cmp::Ordering::Greater => Self::Growing,
            std::cmp::Ordering::Less => Self::Declining,
            std::cmp::Ordering::Equal => Self::Stable,
        }
    }
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyLibraryItem {
    pub slug: String,
    pub display_name: String,
    pub category_slug: String,
    pub category_name: String,
    pub adoption_count: u64,
    pub net_change: i64,
    pub trend: PublicTechnologyTrend,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyCategory {
    pub slug: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyLibraryPage {
    pub items: Vec<PublicTechnologyLibraryItem>,
    pub categories: Vec<PublicTechnologyCategory>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDomainSummary {
    pub canonical_domain: String,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_crawled_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyHistoryPoint {
    pub day: String,
    pub net_change: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicRelatedTechnology {
    pub slug: String,
    pub display_name: String,
    pub category_slug: String,
    pub category_name: String,
    pub shared_domain_count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyDomainPage {
    pub items: Vec<PublicDomainSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyProfile {
    pub slug: String,
    pub display_name: String,
    pub category_slug: String,
    pub category_name: String,
    pub adoption_count: u64,
    pub net_change: i64,
    pub trend: PublicTechnologyTrend,
    pub history: Vec<PublicTechnologyHistoryPoint>,
    pub related_technologies: Vec<PublicRelatedTechnology>,
    pub domains: PublicTechnologyDomainPage,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDomainSearchHit {
    pub canonical_domain: String,
    pub technology_slugs: Vec<String>,
    pub category_slugs: Vec<String>,
    pub country_code: Option<String>,
    pub max_confidence: u8,
    pub last_crawled_at: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDomainSearchPage {
    pub results: Vec<PublicDomainSearchHit>,
    pub estimated_total_hits: u64,
    pub facets: PublicDomainSearchFacets,
    pub limit: usize,
    pub offset: usize,
    pub query_at: String,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicDomainSearchFacets {
    pub technology: BTreeMap<String, u64>,
    pub category: BTreeMap<String, u64>,
    pub country: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicComparisonCell {
    pub category_slug: String,
    pub canonical_domain: String,
    pub technology_slug: Option<String>,
    pub state: String,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub last_observed_at: Option<OffsetDateTime>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicComparisonResponse {
    pub domains: Vec<String>,
    pub cells: Vec<PublicComparisonCell>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsOverview {
    pub domain_count: u64,
    pub technology_count: u64,
    pub current_detection_count: u64,
    pub country_count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAdoption {
    pub slug: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicAdoptionHistoryStatus {
    Ready,
    InsufficientHistory,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAdoptionPoint {
    pub day: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAdoptionSeries {
    pub slug: String,
    pub display_name: String,
    pub points: Vec<PublicAdoptionPoint>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAdoptionHistory {
    pub status: PublicAdoptionHistoryStatus,
    pub available_from: Option<String>,
    pub available_to: Option<String>,
    pub series: Vec<PublicAdoptionSeries>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicTechnologyMigration {
    pub from_slug: String,
    pub from_display_name: String,
    pub to_slug: String,
    pub to_display_name: String,
    pub domain_count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsDomain {
    pub canonical_domain: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub first_indexed_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicFrequentCrawlDomain {
    pub canonical_domain: String,
    pub crawl_count: u64,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub last_crawled_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsDiscovery {
    pub large_migrations: Vec<PublicTechnologyMigration>,
    pub newest_domains: Vec<PublicAnalyticsDomain>,
    pub frequently_crawled_domains: Vec<PublicFrequentCrawlDomain>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsRank {
    pub slug: String,
    pub display_name: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicCountryRank {
    pub country_code: String,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsRankings {
    pub technologies: Vec<PublicAnalyticsRank>,
    pub categories: Vec<PublicAnalyticsRank>,
    pub countries: Vec<PublicCountryRank>,
    pub providers: Vec<PublicAnalyticsRank>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsTechnologyMover {
    pub slug: String,
    pub display_name: String,
    pub category_slug: String,
    pub category_name: String,
    pub adoption_count: u64,
    pub net_change: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsProviderMover {
    pub slug: String,
    pub display_name: String,
    pub adoption_count: u64,
    pub net_change: i64,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicAnalyticsMovers {
    pub growing_technologies: Vec<PublicAnalyticsTechnologyMover>,
    pub declining_technologies: Vec<PublicAnalyticsTechnologyMover>,
    pub growing_providers: Vec<PublicAnalyticsProviderMover>,
    pub declining_providers: Vec<PublicAnalyticsProviderMover>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicProviderDomainPage {
    pub items: Vec<PublicDomainSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicProviderProfile {
    pub slug: String,
    pub display_name: String,
    pub adoption_count: u64,
    pub net_change: i64,
    pub trend: PublicTechnologyTrend,
    pub technologies: Vec<PublicTechnologyLibraryItem>,
    pub domains: PublicProviderDomainPage,
}

#[derive(Clone, Debug, Serialize, ToSchema, PartialEq, Eq)]
pub struct PublicChangeTrend {
    pub day: String,
    pub added: u64,
    pub removed: u64,
    pub migrated: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PublicPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait PublicReadOperations: Send + Sync {
    async fn domain_profile(
        &self,
        domain: &CanonicalDomain,
    ) -> Result<Option<PublicDomainProfile>, PublicReadError>;
    async fn domain_changes(
        &self,
        domain: &CanonicalDomain,
        cursor: Option<OffsetDateTime>,
        limit: usize,
    ) -> Result<PublicPage<PublicChange>, PublicReadError>;
    async fn domain_crawls(
        &self,
        domain: &CanonicalDomain,
        cursor: Option<OffsetDateTime>,
        limit: usize,
    ) -> Result<PublicPage<PublicCrawl>, PublicReadError>;
    async fn crawl_detail(
        &self,
        domain: &CanonicalDomain,
        crawl_id: &str,
    ) -> Result<Option<PublicCrawlDetail>, PublicReadError>;
    async fn technologies(
        &self,
        category: Option<&str>,
        trend: Option<PublicTechnologyTrend>,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<PublicTechnologyLibraryPage, PublicReadError>;
    async fn technology_domains(
        &self,
        technology: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Option<PublicPage<PublicDomainSummary>>, PublicReadError>;
    async fn technology_profile(
        &self,
        technology: &str,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<Option<PublicTechnologyProfile>, PublicReadError>;
    async fn comparison(
        &self,
        domains: &[CanonicalDomain],
        stale_before: OffsetDateTime,
    ) -> Result<Vec<PublicComparisonCell>, PublicReadError>;
    async fn analytics_overview(&self) -> Result<PublicAnalyticsOverview, PublicReadError>;
    async fn adoption(&self) -> Result<Vec<PublicAdoption>, PublicReadError>;
    async fn adoption_history(
        &self,
        since: time::Date,
        technology: Option<&str>,
    ) -> Result<PublicAdoptionHistory, PublicReadError>;
    async fn analytics_rankings(
        &self,
        limit: usize,
    ) -> Result<PublicAnalyticsRankings, PublicReadError>;
    async fn analytics_movers(
        &self,
        since: OffsetDateTime,
        limit: usize,
    ) -> Result<PublicAnalyticsMovers, PublicReadError>;
    async fn change_trends(
        &self,
        since: OffsetDateTime,
    ) -> Result<Vec<PublicChangeTrend>, PublicReadError>;
    async fn analytics_discovery(
        &self,
        since: OffsetDateTime,
        limit: usize,
    ) -> Result<PublicAnalyticsDiscovery, PublicReadError>;
    async fn provider_profile(
        &self,
        provider: &str,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<Option<PublicProviderProfile>, PublicReadError>;
}

#[async_trait]
pub trait PublicRefreshOperations: Send + Sync {
    /// Records a public request and makes the domain eligible for the scheduler. This boundary
    /// deliberately does not enqueue or publish a crawl job directly.
    async fn request_refresh(
        &self,
        domain: &CanonicalDomain,
        requested_at: OffsetDateTime,
        cooldown: time::Duration,
    ) -> Result<PublicRefreshRequest, PublicRefreshError>;
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PublicRefreshError {
    #[error("public domain was not found")]
    NotFound,
    #[error("public refresh is disabled for this domain")]
    Disabled,
    #[error("public refresh request is rate limited")]
    RateLimited { retry_after_seconds: u64 },
    #[error("public refresh persistence is unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum PublicReadError {
    #[error("public read storage is unavailable")]
    Unavailable,
}
