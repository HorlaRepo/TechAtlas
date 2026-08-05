use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// Authoritative PostgreSQL data used to build one replaceable domain search document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SearchDomainProjection {
    pub domain_id: Uuid,
    pub canonical_domain: String,
    pub technologies: Vec<SearchTechnologyProjection>,
    pub last_crawled_at: Option<OffsetDateTime>,
    pub country_code: Option<String>,
    pub updated_at: OffsetDateTime,
}

/// Current technology information suitable for search filtering and display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SearchTechnologyProjection {
    pub slug: String,
    pub category_slug: String,
    pub confidence: u8,
    pub last_observed_at: OffsetDateTime,
}
