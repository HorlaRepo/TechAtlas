#![forbid(unsafe_code)]

use async_trait::async_trait;
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use techatlas_common::{DependencyProbe, DependencyStatus};
use techatlas_models::SearchDomainProjection;
use thiserror::Error;
use time::OffsetDateTime;
use url::Url;

pub const DOMAIN_INDEX_UID: &str = "techatlas-domains-v1";
const FACET_FIELDS: [&str; 3] = ["technology_slugs", "category_slugs", "country_code"];
const MAX_FACET_VALUES: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DomainSearchDocument {
    pub id: String,
    pub domain: String,
    pub technology_slugs: Vec<String>,
    pub category_slugs: Vec<String>,
    pub technologies: Vec<SearchTechnologyDocument>,
    pub last_crawled_at: Option<OffsetDateTime>,
    pub last_crawled_at_unix: Option<i64>,
    pub country_code: Option<String>,
    pub max_confidence: u8,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SearchTechnologyDocument {
    pub slug: String,
    pub category_slug: String,
    pub confidence: u8,
    pub last_observed_at: OffsetDateTime,
}

impl From<SearchDomainProjection> for DomainSearchDocument {
    fn from(projection: SearchDomainProjection) -> Self {
        let mut technologies = projection
            .technologies
            .into_iter()
            .map(|technology| SearchTechnologyDocument {
                slug: technology.slug,
                category_slug: technology.category_slug,
                confidence: technology.confidence,
                last_observed_at: technology.last_observed_at,
            })
            .collect::<Vec<_>>();
        technologies.sort_by(|left, right| left.slug.cmp(&right.slug));
        let technology_slugs = technologies
            .iter()
            .map(|technology| technology.slug.clone())
            .collect();
        let mut category_slugs = technologies
            .iter()
            .map(|technology| technology.category_slug.clone())
            .collect::<Vec<_>>();
        category_slugs.sort();
        category_slugs.dedup();
        let max_confidence = technologies
            .iter()
            .map(|technology| technology.confidence)
            .max()
            .unwrap_or(0);
        Self {
            id: projection.domain_id.to_string(),
            domain: projection.canonical_domain,
            technology_slugs,
            category_slugs,
            technologies,
            last_crawled_at: projection.last_crawled_at,
            last_crawled_at_unix: projection
                .last_crawled_at
                .map(|value| value.unix_timestamp()),
            country_code: projection.country_code,
            max_confidence,
            updated_at: projection.updated_at,
        }
    }
}

#[async_trait]
pub trait DomainSearchIndex: Send + Sync {
    async fn recreate(&self) -> Result<(), SearchIndexError>;
    async fn upsert(&self, documents: Vec<DomainSearchDocument>) -> Result<(), SearchIndexError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DomainSearchQuery {
    pub query: Option<String>,
    pub technology_slugs: Vec<String>,
    pub category_slugs: Vec<String>,
    pub country_codes: Vec<String>,
    pub crawled_since: Option<OffsetDateTime>,
    pub min_confidence: Option<u8>,
    pub limit: usize,
    pub offset: usize,
    pub sort: DomainSearchSort,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainSearchSort {
    Relevance,
    LastCrawledDesc,
    UpdatedDesc,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DomainSearchHit {
    pub canonical_domain: String,
    pub technology_slugs: Vec<String>,
    pub category_slugs: Vec<String>,
    pub country_code: Option<String>,
    pub max_confidence: u8,
    pub last_crawled_at: Option<OffsetDateTime>,
    pub updated_at: OffsetDateTime,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct DomainSearchPage {
    pub hits: Vec<DomainSearchHit>,
    pub estimated_total_hits: u64,
    pub facets: DomainSearchFacets,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct DomainSearchFacets {
    pub technology: BTreeMap<String, u64>,
    pub category: BTreeMap<String, u64>,
    pub country: BTreeMap<String, u64>,
}

#[async_trait]
pub trait DomainSearch: Send + Sync {
    async fn search(&self, query: DomainSearchQuery) -> Result<DomainSearchPage, SearchIndexError>;
}

pub struct MeilisearchDomainIndex {
    client: Client,
    base_url: Url,
    master_key: SecretString,
    task_poll_interval: Duration,
}

impl MeilisearchDomainIndex {
    pub fn new(
        base_url: &Url,
        master_key: &SecretString,
        timeout: Duration,
    ) -> Result<Self, SearchIndexError> {
        Ok(Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|_| SearchIndexError::Client)?,
            base_url: base_url.clone(),
            master_key: master_key.clone(),
            task_poll_interval: Duration::from_millis(100),
        })
    }

    async fn request_task(&self, request: reqwest::RequestBuilder) -> Result<(), SearchIndexError> {
        let response = request
            .bearer_auth(self.master_key.expose_secret())
            .send()
            .await
            .map_err(|_| SearchIndexError::Unavailable)?;
        if !response.status().is_success() {
            return Err(SearchIndexError::Rejected);
        }
        let task = response
            .json::<Value>()
            .await
            .map_err(|_| SearchIndexError::Rejected)?;
        if let Some(task_uid) = task.get("taskUid").and_then(Value::as_u64) {
            self.wait_for_task(task_uid).await?;
        }
        Ok(())
    }

    async fn wait_for_task(&self, task_uid: u64) -> Result<(), SearchIndexError> {
        let url = self
            .base_url
            .join(&format!("tasks/{task_uid}"))
            .map_err(|_| SearchIndexError::InvalidBaseUrl)?;
        loop {
            let response = self
                .client
                .get(url.clone())
                .bearer_auth(self.master_key.expose_secret())
                .send()
                .await
                .map_err(|_| SearchIndexError::Unavailable)?;
            if !response.status().is_success() {
                return Err(SearchIndexError::Rejected);
            }
            let task = response
                .json::<Value>()
                .await
                .map_err(|_| SearchIndexError::Rejected)?;
            match task.get("status").and_then(Value::as_str) {
                Some("succeeded") => return Ok(()),
                Some("failed") | Some("canceled") => return Err(SearchIndexError::Rejected),
                _ => tokio::time::sleep(self.task_poll_interval).await,
            }
        }
    }

    fn index_url(&self) -> Result<Url, SearchIndexError> {
        self.base_url
            .join(&format!("indexes/{DOMAIN_INDEX_UID}"))
            .map_err(|_| SearchIndexError::InvalidBaseUrl)
    }
}

#[async_trait]
impl DomainSearch for MeilisearchDomainIndex {
    async fn search(&self, query: DomainSearchQuery) -> Result<DomainSearchPage, SearchIndexError> {
        let url = self
            .base_url
            .join(&format!("indexes/{DOMAIN_INDEX_UID}/search"))
            .map_err(|_| SearchIndexError::InvalidBaseUrl)?;
        let mut filters = Vec::new();
        filters.extend(
            query
                .technology_slugs
                .iter()
                .map(|value| format!("technology_slugs = '{}'", escape_filter(value))),
        );
        filters.extend(
            query
                .category_slugs
                .iter()
                .map(|value| format!("category_slugs = '{}'", escape_filter(value))),
        );
        filters.extend(
            query
                .country_codes
                .iter()
                .map(|value| format!("country_code = '{}'", escape_filter(value))),
        );
        if let Some(crawled_since) = query.crawled_since {
            filters.push(format!(
                "last_crawled_at_unix >= {}",
                crawled_since.unix_timestamp()
            ));
        }
        if let Some(min_confidence) = query.min_confidence {
            filters.push(format!("max_confidence >= {min_confidence}"));
        }
        let sort = match query.sort {
            DomainSearchSort::Relevance => None,
            DomainSearchSort::LastCrawledDesc => Some("last_crawled_at:desc"),
            DomainSearchSort::UpdatedDesc => Some("updated_at:desc"),
        };
        let response = self
            .client
            .post(url)
            .bearer_auth(self.master_key.expose_secret())
            .json(&search_request_body(&query, &filters, sort))
            .send()
            .await
            .map_err(|_| SearchIndexError::Unavailable)?;
        if !response.status().is_success() {
            return Err(SearchIndexError::Rejected);
        }
        let response = response
            .json::<MeiliSearchResponse>()
            .await
            .map_err(|_| SearchIndexError::Rejected)?;
        let hits = response
            .hits
            .into_iter()
            .map(|hit| DomainSearchHit {
                canonical_domain: hit.domain,
                technology_slugs: hit.technology_slugs,
                category_slugs: hit.category_slugs,
                country_code: hit.country_code,
                max_confidence: hit.max_confidence,
                last_crawled_at: hit.last_crawled_at,
                updated_at: hit.updated_at,
            })
            .collect();
        Ok(DomainSearchPage {
            hits,
            estimated_total_hits: response.estimated_total_hits,
            facets: DomainSearchFacets {
                technology: response.facet_distribution.technology_slugs,
                category: response.facet_distribution.category_slugs,
                country: response.facet_distribution.country_code,
            },
        })
    }
}

fn escape_filter(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

fn search_request_body(query: &DomainSearchQuery, filters: &[String], sort: Option<&str>) -> Value {
    json!({
        "q": query.query.clone().unwrap_or_default(),
        "filter": filters,
        "facets": FACET_FIELDS,
        "limit": query.limit,
        "offset": query.offset,
        "sort": sort.into_iter().collect::<Vec<_>>(),
    })
}

#[derive(Deserialize)]
struct MeiliSearchResponse {
    hits: Vec<MeiliSearchHit>,
    #[serde(default, rename = "estimatedTotalHits")]
    estimated_total_hits: u64,
    #[serde(default, rename = "facetDistribution")]
    facet_distribution: MeiliFacetDistribution,
}

#[derive(Default, Deserialize)]
struct MeiliFacetDistribution {
    #[serde(default)]
    technology_slugs: BTreeMap<String, u64>,
    #[serde(default)]
    category_slugs: BTreeMap<String, u64>,
    #[serde(default)]
    country_code: BTreeMap<String, u64>,
}
#[derive(Deserialize)]
struct MeiliSearchHit {
    domain: String,
    #[serde(default)]
    technology_slugs: Vec<String>,
    #[serde(default)]
    category_slugs: Vec<String>,
    country_code: Option<String>,
    max_confidence: u8,
    last_crawled_at: Option<OffsetDateTime>,
    updated_at: OffsetDateTime,
}

#[async_trait]
impl DomainSearchIndex for MeilisearchDomainIndex {
    async fn recreate(&self) -> Result<(), SearchIndexError> {
        let index_url = self.index_url()?;
        let existing = self
            .client
            .get(index_url.clone())
            .bearer_auth(self.master_key.expose_secret())
            .send()
            .await
            .map_err(|_| SearchIndexError::Unavailable)?;
        if should_delete_existing_index(existing.status())? {
            let delete = self
                .client
                .delete(index_url)
                .bearer_auth(self.master_key.expose_secret())
                .send()
                .await
                .map_err(|_| SearchIndexError::Unavailable)?;
            if !delete.status().is_success() {
                return Err(SearchIndexError::Rejected);
            }
            let task = delete
                .json::<Value>()
                .await
                .map_err(|_| SearchIndexError::Rejected)?;
            if let Some(task_uid) = task.get("taskUid").and_then(Value::as_u64) {
                self.wait_for_task(task_uid).await?;
            }
        }
        let create_url = self
            .base_url
            .join("indexes")
            .map_err(|_| SearchIndexError::InvalidBaseUrl)?;
        self.request_task(
            self.client
                .post(create_url)
                .json(&json!({"uid": DOMAIN_INDEX_UID, "primaryKey": "id"})),
        )
        .await?;
        let settings_url = self
            .base_url
            .join(&format!("indexes/{DOMAIN_INDEX_UID}/settings"))
            .map_err(|_| SearchIndexError::InvalidBaseUrl)?;
        self.request_task(self.client.patch(settings_url).json(&json!({
            "searchableAttributes": ["domain", "technology_slugs"],
            "filterableAttributes": ["technology_slugs", "category_slugs", "country_code", "max_confidence", "last_crawled_at_unix"],
            "sortableAttributes": ["last_crawled_at", "updated_at"],
            "faceting": {"maxValuesPerFacet": MAX_FACET_VALUES}
        })))
        .await
    }

    async fn upsert(&self, documents: Vec<DomainSearchDocument>) -> Result<(), SearchIndexError> {
        if documents.is_empty() {
            return Ok(());
        }
        let url = self
            .base_url
            .join(&format!(
                "indexes/{DOMAIN_INDEX_UID}/documents?primaryKey=id"
            ))
            .map_err(|_| SearchIndexError::InvalidBaseUrl)?;
        self.request_task(self.client.post(url).json(&documents))
            .await
    }
}

fn should_delete_existing_index(status: reqwest::StatusCode) -> Result<bool, SearchIndexError> {
    if status.is_success() {
        Ok(true)
    } else if status == reqwest::StatusCode::NOT_FOUND {
        Ok(false)
    } else {
        Err(SearchIndexError::Rejected)
    }
}

#[derive(Debug, Error)]
pub enum SearchIndexError {
    #[error("Meilisearch base URL is invalid")]
    InvalidBaseUrl,
    #[error("Meilisearch client could not be initialised")]
    Client,
    #[error("Meilisearch is unavailable")]
    Unavailable,
    #[error("Meilisearch rejected an index task")]
    Rejected,
}

#[cfg(test)]
mod tests {
    use super::*;
    use techatlas_models::{SearchDomainProjection, SearchTechnologyProjection};
    use time::OffsetDateTime;
    use uuid::Uuid;

    #[test]
    fn domain_documents_sort_and_deduplicate_filter_fields() {
        let document = DomainSearchDocument::from(SearchDomainProjection {
            domain_id: Uuid::from_u128(1),
            canonical_domain: "example.test".to_owned(),
            technologies: vec![
                SearchTechnologyProjection {
                    slug: "stripe".to_owned(),
                    category_slug: "payment-provider".to_owned(),
                    confidence: 95,
                    last_observed_at: OffsetDateTime::UNIX_EPOCH,
                },
                SearchTechnologyProjection {
                    slug: "nextjs".to_owned(),
                    category_slug: "framework".to_owned(),
                    confidence: 90,
                    last_observed_at: OffsetDateTime::UNIX_EPOCH,
                },
            ],
            last_crawled_at: Some(OffsetDateTime::UNIX_EPOCH),
            country_code: None,
            updated_at: OffsetDateTime::UNIX_EPOCH,
        });

        assert_eq!(document.technology_slugs, ["nextjs", "stripe"]);
        assert_eq!(document.category_slugs, ["framework", "payment-provider"]);
        assert_eq!(document.max_confidence, 95);
    }

    #[test]
    fn deserializes_bounded_search_facet_distributions() {
        let response = serde_json::from_value::<MeiliSearchResponse>(json!({
            "hits": [],
            "estimatedTotalHits": 4,
            "facetDistribution": {
                "technology_slugs": { "react": 3, "nextjs": 2 },
                "category_slugs": { "frontend-framework": 4 },
                "country_code": { "NG": 3 }
            }
        }))
        .expect("Meilisearch facet response should deserialize");

        assert_eq!(response.estimated_total_hits, 4);
        assert_eq!(response.facet_distribution.technology_slugs["react"], 3);
        assert_eq!(
            response.facet_distribution.category_slugs["frontend-framework"],
            4
        );
        assert_eq!(response.facet_distribution.country_code["NG"], 3);
    }

    #[test]
    fn search_requests_the_supported_facet_distributions() {
        let body = search_request_body(
            &DomainSearchQuery {
                query: Some("example".to_owned()),
                technology_slugs: vec![],
                category_slugs: vec![],
                country_codes: vec![],
                crawled_since: None,
                min_confidence: None,
                limit: 50,
                offset: 0,
                sort: DomainSearchSort::Relevance,
            },
            &["technology_slugs = 'react'".to_owned()],
            None,
        );

        assert_eq!(body["facets"], json!(FACET_FIELDS));
        assert_eq!(body["filter"], json!(["technology_slugs = 'react'"]));
    }

    #[test]
    fn missing_index_does_not_trigger_a_failed_deletion_task() {
        assert!(matches!(
            should_delete_existing_index(reqwest::StatusCode::NOT_FOUND),
            Ok(false)
        ));
    }
}

pub struct MeilisearchProbe {
    client: Client,
    health_url: Url,
    master_key: SecretString,
}

impl MeilisearchProbe {
    pub fn new(
        base_url: &Url,
        master_key: &SecretString,
        timeout: Duration,
    ) -> Result<Self, MeilisearchProbeError> {
        let health_url = base_url
            .join("health")
            .map_err(|_| MeilisearchProbeError::InvalidBaseUrl)?;

        Ok(Self {
            client: Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|_| MeilisearchProbeError::ClientInitialisation)?,
            health_url,
            master_key: master_key.clone(),
        })
    }
}

#[async_trait]
impl DependencyProbe for MeilisearchProbe {
    async fn check(&self) -> DependencyStatus {
        match self
            .client
            .get(self.health_url.clone())
            .bearer_auth(self.master_key.expose_secret())
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => DependencyStatus::Ready,
            _ => DependencyStatus::Unavailable,
        }
    }
}

#[derive(Debug, Error)]
pub enum MeilisearchProbeError {
    #[error("invalid Meilisearch base URL")]
    InvalidBaseUrl,
    #[error("could not initialise Meilisearch HTTP client")]
    ClientInitialisation,
}
