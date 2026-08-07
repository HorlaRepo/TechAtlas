use crate::{
    admin_domains::{ErrorBody, ErrorEnvelope},
    app::AppState,
};
use axum::{
    Json, Router,
    extract::{Path, Query, RawQuery, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use techatlas_models::{
    CanonicalDomain, PublicAdoption, PublicAdoptionHistory, PublicAnalyticsDiscovery,
    PublicAnalyticsMovers, PublicAnalyticsOverview, PublicAnalyticsRankings, PublicChangePage,
    PublicChangeTrend, PublicComparisonResponse, PublicCrawlDetail, PublicCrawlPage,
    PublicDomainProfile, PublicDomainSearchFacets, PublicDomainSearchHit, PublicDomainSearchPage,
    PublicProviderProfile, PublicReadError, PublicRefreshError, PublicRefreshRequest,
    PublicTechnologyLibraryPage, PublicTechnologyTrend,
};
use techatlas_search::{DomainSearchPage, DomainSearchQuery, DomainSearchSort};
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::IntoParams;

const DEFAULT_PAGE_SIZE: usize = 50;
const MAX_PAGE_SIZE: usize = 100;
const TECHNOLOGY_TREND_WINDOW_DAYS: i64 = 30;
const ANALYTICS_RANK_LIMIT: usize = 10;
const ANALYTICS_MOVER_LIMIT: usize = 5;
const ANALYTICS_DISCOVERY_LIMIT: usize = 10;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/public/search/domains", get(search_domains))
        .route(
            "/api/v1/public/domains/{canonical_domain}",
            get(domain_profile).post(request_refresh),
        )
        .route(
            "/api/v1/public/domains/{canonical_domain}/changes",
            get(domain_changes),
        )
        .route(
            "/api/v1/public/domains/{canonical_domain}/crawls",
            get(domain_crawls),
        )
        .route(
            "/api/v1/public/domains/{canonical_domain}/crawls/{crawl_id}",
            get(crawl_detail),
        )
        .route("/api/v1/public/technologies", get(technologies))
        .route(
            "/api/v1/public/technologies/{technology_slug}",
            get(technology_domains),
        )
        .route(
            "/api/v1/public/technologies/{technology_slug}/profile",
            get(technology_profile),
        )
        .route("/api/v1/public/compare", get(compare))
        .route("/api/v1/public/analytics/overview", get(analytics_overview))
        .route("/api/v1/public/analytics/adoption", get(analytics_adoption))
        .route(
            "/api/v1/public/analytics/adoption-history",
            get(analytics_adoption_history),
        )
        .route("/api/v1/public/analytics/rankings", get(analytics_rankings))
        .route("/api/v1/public/analytics/movers", get(analytics_movers))
        .route("/api/v1/public/analytics/changes", get(analytics_changes))
        .route(
            "/api/v1/public/analytics/discovery",
            get(analytics_discovery),
        )
        .route(
            "/api/v1/public/providers/{provider_slug}",
            get(provider_profile),
        )
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TechnologyLibraryQuery {
    pub category: Option<String>,
    pub trend: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    pub q: Option<String>,
    pub technology: Option<Vec<String>>,
    pub category: Option<Vec<String>>,
    pub country: Option<Vec<String>>,
    pub crawled_since: Option<i64>,
    pub min_confidence: Option<u8>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CompareQuery {
    pub domain: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrendQuery {
    pub since_days: Option<u16>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AdoptionHistoryQuery {
    pub since_days: Option<u16>,
    pub technology: Option<String>,
}

#[utoipa::path(get, path = "/api/v1/public/search/domains", tag = "public-search", params(SearchQuery), responses((status = 200, description = "Search result page", body = PublicDomainSearchPage), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn search_domains(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<PublicDomainSearchPage>, PublicApiError> {
    let query = parse_search_query(raw_query.as_deref())?;
    let limit = page_limit(query.limit)?;
    let offset = query.offset.unwrap_or(0);
    if offset > 10_000 {
        return Err(PublicApiError::Validation);
    }
    let sort = match query.sort.as_deref().unwrap_or("relevance") {
        "relevance" => DomainSearchSort::Relevance,
        "last_crawled_desc" => DomainSearchSort::LastCrawledDesc,
        "updated_desc" => DomainSearchSort::UpdatedDesc,
        _ => return Err(PublicApiError::Validation),
    };
    let q = query
        .q
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_owned());
    if q.as_ref().is_some_and(|value| value.len() > 200) {
        return Err(PublicApiError::Validation);
    }
    let technology_slugs = validate_slugs(query.technology.unwrap_or_default())?;
    let category_slugs = validate_slugs(query.category.unwrap_or_default())?;
    let country_codes = validate_countries(query.country.unwrap_or_default())?;
    let crawled_since = query
        .crawled_since
        .map(OffsetDateTime::from_unix_timestamp)
        .transpose()
        .map_err(|_| PublicApiError::Validation)?;
    let page = state
        .public_search
        .search(DomainSearchQuery {
            query: q,
            technology_slugs,
            category_slugs,
            country_codes,
            crawled_since,
            min_confidence: query.min_confidence,
            limit,
            offset,
            sort,
        })
        .await
        .map_err(|_| PublicApiError::Unavailable)?;
    Ok(Json(search_page_response(page, limit, offset)?))
}

fn parse_search_query(raw_query: Option<&str>) -> Result<SearchQuery, PublicApiError> {
    let mut query = SearchQuery {
        q: None,
        technology: None,
        category: None,
        country: None,
        crawled_since: None,
        min_confidence: None,
        limit: None,
        offset: None,
        sort: None,
    };
    let Some(raw_query) = raw_query else {
        return Ok(query);
    };

    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        match key.as_ref() {
            "q" => set_once(&mut query.q, value.into_owned())?,
            "technology" => query
                .technology
                .get_or_insert_with(Vec::new)
                .push(value.into_owned()),
            "category" => query
                .category
                .get_or_insert_with(Vec::new)
                .push(value.into_owned()),
            "country" => query
                .country
                .get_or_insert_with(Vec::new)
                .push(value.into_owned()),
            "crawled_since" => set_once(
                &mut query.crawled_since,
                value.parse().map_err(|_| PublicApiError::Validation)?,
            )?,
            "min_confidence" => set_once(
                &mut query.min_confidence,
                value.parse().map_err(|_| PublicApiError::Validation)?,
            )?,
            "limit" => set_once(
                &mut query.limit,
                value.parse().map_err(|_| PublicApiError::Validation)?,
            )?,
            "offset" => set_once(
                &mut query.offset,
                value.parse().map_err(|_| PublicApiError::Validation)?,
            )?,
            "sort" => set_once(&mut query.sort, value.into_owned())?,
            _ => {}
        }
    }

    Ok(query)
}

fn set_once<T>(target: &mut Option<T>, value: T) -> Result<(), PublicApiError> {
    if target.replace(value).is_some() {
        return Err(PublicApiError::Validation);
    }
    Ok(())
}

fn search_page_response(
    page: DomainSearchPage,
    limit: usize,
    offset: usize,
) -> Result<PublicDomainSearchPage, PublicApiError> {
    let results = page
        .hits
        .into_iter()
        .map(|hit| {
            Ok(PublicDomainSearchHit {
                canonical_domain: hit.canonical_domain,
                technology_slugs: hit.technology_slugs,
                category_slugs: hit.category_slugs,
                country_code: hit.country_code,
                max_confidence: hit.max_confidence,
                last_crawled_at: hit.last_crawled_at.map(format_timestamp).transpose()?,
                updated_at: format_timestamp(hit.updated_at)?,
            })
        })
        .collect::<Result<Vec<_>, PublicApiError>>()?;
    Ok(PublicDomainSearchPage {
        results,
        estimated_total_hits: page.estimated_total_hits,
        facets: PublicDomainSearchFacets {
            technology: page.facets.technology,
            category: page.facets.category,
            country: page.facets.country,
        },
        limit,
        offset,
        query_at: format_timestamp(OffsetDateTime::now_utc())?,
    })
}

fn format_timestamp(value: OffsetDateTime) -> Result<String, PublicApiError> {
    value
        .format(&Rfc3339)
        .map_err(|_| PublicApiError::Unavailable)
}

#[utoipa::path(get, path = "/api/v1/public/domains/{canonical_domain}", tag = "public-domains", params(("canonical_domain" = String, Path)), responses((status = 200, body = PublicDomainProfile), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn domain_profile(
    State(state): State<AppState>,
    Path(canonical_domain): Path<String>,
) -> Result<Json<PublicDomainProfile>, PublicApiError> {
    let domain = domain(&canonical_domain)?;
    let profile = state
        .public_reads
        .domain_profile(&domain)
        .await
        .map_err(map_read)?
        .ok_or(PublicApiError::NotFound)?;
    Ok(Json(profile))
}

#[utoipa::path(get, path = "/api/v1/public/domains/{canonical_domain}/changes", tag = "public-domains", params(("canonical_domain" = String, Path), PageQuery), responses((status = 200, body = PublicChangePage), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn domain_changes(
    State(state): State<AppState>,
    Path(canonical_domain): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PublicChangePage>, PublicApiError> {
    let domain = domain(&canonical_domain)?;
    ensure_domain(&state, &domain).await?;
    let page = state
        .public_reads
        .domain_changes(&domain, parse_time(query.cursor)?, page_limit(query.limit)?)
        .await
        .map_err(map_read)?;
    Ok(Json(page.into()))
}

#[utoipa::path(get, path = "/api/v1/public/domains/{canonical_domain}/crawls", tag = "public-domains", params(("canonical_domain" = String, Path), PageQuery), responses((status = 200, body = PublicCrawlPage), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn domain_crawls(
    State(state): State<AppState>,
    Path(canonical_domain): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PublicCrawlPage>, PublicApiError> {
    let domain = domain(&canonical_domain)?;
    ensure_domain(&state, &domain).await?;
    let page = state
        .public_reads
        .domain_crawls(&domain, parse_time(query.cursor)?, page_limit(query.limit)?)
        .await
        .map_err(map_read)?;
    Ok(Json(page.into()))
}

#[utoipa::path(get, path = "/api/v1/public/domains/{canonical_domain}/crawls/{crawl_id}", tag = "public-domains", params(("canonical_domain" = String, Path), ("crawl_id" = String, Path)), responses((status = 200, body = PublicCrawlDetail), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn crawl_detail(
    State(state): State<AppState>,
    Path((canonical_domain, crawl_id)): Path<(String, String)>,
) -> Result<Json<PublicCrawlDetail>, PublicApiError> {
    let domain = domain(&canonical_domain)?;
    let detail = state
        .public_reads
        .crawl_detail(&domain, &crawl_id)
        .await
        .map_err(map_read)?
        .ok_or(PublicApiError::NotFound)?;
    Ok(Json(detail))
}

#[utoipa::path(post, path = "/api/v1/public/domains/{canonical_domain}", tag = "public-domains", params(("canonical_domain" = String, Path)), responses((status = 202, description = "Refresh request recorded for scheduler processing", body = PublicRefreshRequest), (status = 404, body = ErrorEnvelope), (status = 409, description = "Refresh is disabled for the domain", body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope, headers(("Retry-After" = u64, description = "Seconds before another request may be accepted"))), (status = 503, body = ErrorEnvelope)))]
pub async fn request_refresh(
    State(state): State<AppState>,
    Path(canonical_domain): Path<String>,
) -> Result<(StatusCode, Json<PublicRefreshRequest>), PublicApiError> {
    let domain = domain(&canonical_domain)?;
    let cooldown = Duration::hours(i64::from(state.public_refresh_cooldown_hours));
    let request = state
        .public_refreshes
        .request_refresh(&domain, OffsetDateTime::now_utc(), cooldown)
        .await
        .map_err(map_refresh)?;
    Ok((StatusCode::ACCEPTED, Json(request)))
}

#[utoipa::path(get, path = "/api/v1/public/technologies", tag = "public-technologies", params(TechnologyLibraryQuery), responses((status = 200, body = PublicTechnologyLibraryPage), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn technologies(
    State(state): State<AppState>,
    Query(query): Query<TechnologyLibraryQuery>,
) -> Result<Json<PublicTechnologyLibraryPage>, PublicApiError> {
    if query
        .category
        .as_deref()
        .is_some_and(|value| !valid_slug(value))
    {
        return Err(PublicApiError::Validation);
    }
    let trend = technology_trend(query.trend.as_deref())?;
    let page = state
        .public_reads
        .technologies(
            query.category.as_deref(),
            trend,
            query.cursor.as_deref(),
            page_limit(query.limit)?,
            OffsetDateTime::now_utc() - Duration::days(TECHNOLOGY_TREND_WINDOW_DAYS),
        )
        .await
        .map_err(map_read)?;
    Ok(Json(page))
}

#[utoipa::path(get, path = "/api/v1/public/technologies/{technology_slug}", tag = "public-technologies", params(("technology_slug" = String, Path), PageQuery), responses((status = 200, body = Object), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn technology_domains(
    State(state): State<AppState>,
    Path(technology_slug): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<serde_json::Value>, PublicApiError> {
    if !valid_slug(&technology_slug) {
        return Err(PublicApiError::Validation);
    }
    let page = state
        .public_reads
        .technology_domains(
            &technology_slug,
            query.cursor.as_deref(),
            page_limit(query.limit)?,
        )
        .await
        .map_err(map_read)?
        .ok_or(PublicApiError::NotFound)?;
    Ok(Json(serde_json::json!(page)))
}

#[utoipa::path(get, path = "/api/v1/public/technologies/{technology_slug}/profile", tag = "public-technologies", params(("technology_slug" = String, Path), PageQuery), responses((status = 200, body = techatlas_models::PublicTechnologyProfile), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn technology_profile(
    State(state): State<AppState>,
    Path(technology_slug): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<techatlas_models::PublicTechnologyProfile>, PublicApiError> {
    if !valid_slug(&technology_slug) {
        return Err(PublicApiError::Validation);
    }
    let profile = state
        .public_reads
        .technology_profile(
            &technology_slug,
            query.cursor.as_deref(),
            page_limit(query.limit)?,
            OffsetDateTime::now_utc() - Duration::days(TECHNOLOGY_TREND_WINDOW_DAYS),
        )
        .await
        .map_err(map_read)?
        .ok_or(PublicApiError::NotFound)?;
    Ok(Json(profile))
}

#[utoipa::path(get, path = "/api/v1/public/compare", tag = "public-comparison", params(CompareQuery), responses((status = 200, body = PublicComparisonResponse), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn compare(
    State(state): State<AppState>,
    RawQuery(raw_query): RawQuery,
) -> Result<Json<PublicComparisonResponse>, PublicApiError> {
    let query = parse_compare_query(raw_query.as_deref())?;
    let values = query.domain.unwrap_or_default();
    if !(2..=10).contains(&values.len()) {
        return Err(PublicApiError::Validation);
    }
    let domains = values
        .iter()
        .map(|value| domain(value))
        .collect::<Result<Vec<_>, _>>()?;
    if domains
        .iter()
        .enumerate()
        .any(|(index, value)| domains.iter().skip(index + 1).any(|other| other == value))
    {
        return Err(PublicApiError::Validation);
    }
    let cells = state
        .public_reads
        .comparison(
            &domains,
            OffsetDateTime::now_utc() - Duration::days(i64::from(state.public_stale_after_days)),
        )
        .await
        .map_err(map_read)?;
    Ok(Json(PublicComparisonResponse {
        domains: domains
            .iter()
            .map(|domain| domain.as_str().to_owned())
            .collect(),
        cells,
    }))
}

fn parse_compare_query(raw_query: Option<&str>) -> Result<CompareQuery, PublicApiError> {
    let mut query = CompareQuery { domain: None };
    let Some(raw_query) = raw_query else {
        return Ok(query);
    };

    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key == "domain" {
            query
                .domain
                .get_or_insert_with(Vec::new)
                .push(value.into_owned());
        }
    }

    Ok(query)
}

#[utoipa::path(get, path = "/api/v1/public/analytics/overview", tag = "public-analytics", responses((status = 200, body = PublicAnalyticsOverview), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_overview(
    State(state): State<AppState>,
) -> Result<Json<PublicAnalyticsOverview>, PublicApiError> {
    Ok(Json(
        state
            .public_reads
            .analytics_overview()
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/adoption", tag = "public-analytics", responses((status = 200, body = [PublicAdoption]), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_adoption(
    State(state): State<AppState>,
) -> Result<Json<Vec<PublicAdoption>>, PublicApiError> {
    Ok(Json(state.public_reads.adoption().await.map_err(map_read)?))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/adoption-history", tag = "public-analytics", params(AdoptionHistoryQuery), responses((status = 200, body = PublicAdoptionHistory), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_adoption_history(
    State(state): State<AppState>,
    Query(query): Query<AdoptionHistoryQuery>,
) -> Result<Json<PublicAdoptionHistory>, PublicApiError> {
    let since = analytics_since(TrendQuery {
        since_days: query.since_days,
    })?
    .date();
    let technology = query.technology.as_deref();
    if technology.is_some_and(|value| !valid_slug(value)) {
        return Err(PublicApiError::Validation);
    }
    Ok(Json(
        state
            .public_reads
            .adoption_history(since, technology)
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/rankings", tag = "public-analytics", responses((status = 200, body = PublicAnalyticsRankings), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_rankings(
    State(state): State<AppState>,
) -> Result<Json<PublicAnalyticsRankings>, PublicApiError> {
    Ok(Json(
        state
            .public_reads
            .analytics_rankings(ANALYTICS_RANK_LIMIT)
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/movers", tag = "public-analytics", params(TrendQuery), responses((status = 200, body = PublicAnalyticsMovers), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_movers(
    State(state): State<AppState>,
    Query(query): Query<TrendQuery>,
) -> Result<Json<PublicAnalyticsMovers>, PublicApiError> {
    let since = analytics_since(query)?;
    Ok(Json(
        state
            .public_reads
            .analytics_movers(since, ANALYTICS_MOVER_LIMIT)
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/changes", tag = "public-analytics", params(TrendQuery), responses((status = 200, body = [PublicChangeTrend]), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_changes(
    State(state): State<AppState>,
    Query(query): Query<TrendQuery>,
) -> Result<Json<Vec<PublicChangeTrend>>, PublicApiError> {
    let since = analytics_since(query)?;
    Ok(Json(
        state
            .public_reads
            .change_trends(since)
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/analytics/discovery", tag = "public-analytics", params(TrendQuery), responses((status = 200, body = PublicAnalyticsDiscovery), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn analytics_discovery(
    State(state): State<AppState>,
    Query(query): Query<TrendQuery>,
) -> Result<Json<PublicAnalyticsDiscovery>, PublicApiError> {
    let since = analytics_since(query)?;
    Ok(Json(
        state
            .public_reads
            .analytics_discovery(since, ANALYTICS_DISCOVERY_LIMIT)
            .await
            .map_err(map_read)?,
    ))
}

#[utoipa::path(get, path = "/api/v1/public/providers/{provider_slug}", tag = "public-providers", params(("provider_slug" = String, Path), PageQuery), responses((status = 200, body = PublicProviderProfile), (status = 404, body = ErrorEnvelope), (status = 422, body = ErrorEnvelope), (status = 503, body = ErrorEnvelope)))]
pub async fn provider_profile(
    State(state): State<AppState>,
    Path(provider_slug): Path<String>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PublicProviderProfile>, PublicApiError> {
    if !valid_slug(&provider_slug) {
        return Err(PublicApiError::Validation);
    }
    let profile = state
        .public_reads
        .provider_profile(
            &provider_slug,
            query.cursor.as_deref(),
            page_limit(query.limit)?,
            OffsetDateTime::now_utc() - Duration::days(TECHNOLOGY_TREND_WINDOW_DAYS),
        )
        .await
        .map_err(map_read)?
        .ok_or(PublicApiError::NotFound)?;
    Ok(Json(profile))
}

async fn ensure_domain(state: &AppState, domain: &CanonicalDomain) -> Result<(), PublicApiError> {
    state
        .public_reads
        .domain_profile(domain)
        .await
        .map_err(map_read)?
        .map(|_| ())
        .ok_or(PublicApiError::NotFound)
}
fn domain(value: &str) -> Result<CanonicalDomain, PublicApiError> {
    CanonicalDomain::parse(value).map_err(|_| PublicApiError::Validation)
}
fn analytics_since(query: TrendQuery) -> Result<OffsetDateTime, PublicApiError> {
    let days = query.since_days.unwrap_or(30);
    if days == 0 || days > 365 {
        return Err(PublicApiError::Validation);
    }
    Ok(OffsetDateTime::now_utc() - Duration::days(i64::from(days)))
}
fn parse_time(value: Option<String>) -> Result<Option<OffsetDateTime>, PublicApiError> {
    value
        .map(|value| {
            value
                .parse::<i64>()
                .ok()
                .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
                .ok_or(PublicApiError::Validation)
        })
        .transpose()
}
fn page_limit(limit: Option<usize>) -> Result<usize, PublicApiError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
    (1..=MAX_PAGE_SIZE)
        .contains(&limit)
        .then_some(limit)
        .ok_or(PublicApiError::Validation)
}
fn technology_trend(value: Option<&str>) -> Result<Option<PublicTechnologyTrend>, PublicApiError> {
    match value {
        None => Ok(None),
        Some("growing") => Ok(Some(PublicTechnologyTrend::Growing)),
        Some("declining") => Ok(Some(PublicTechnologyTrend::Declining)),
        Some("stable") => Ok(Some(PublicTechnologyTrend::Stable)),
        Some(_) => Err(PublicApiError::Validation),
    }
}
fn validate_slugs(values: Vec<String>) -> Result<Vec<String>, PublicApiError> {
    if values.len() > 20 || values.iter().any(|value| !valid_slug(value)) {
        Err(PublicApiError::Validation)
    } else {
        Ok(values)
    }
}
fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
fn validate_countries(values: Vec<String>) -> Result<Vec<String>, PublicApiError> {
    if values.len() > 20
        || values
            .iter()
            .any(|value| value.len() != 2 || !value.bytes().all(|byte| byte.is_ascii_uppercase()))
    {
        Err(PublicApiError::Validation)
    } else {
        Ok(values)
    }
}
fn map_read(_: PublicReadError) -> PublicApiError {
    PublicApiError::Unavailable
}
fn map_refresh(error: PublicRefreshError) -> PublicApiError {
    match error {
        PublicRefreshError::NotFound => PublicApiError::NotFound,
        PublicRefreshError::Disabled => PublicApiError::RefreshUnavailable,
        PublicRefreshError::RateLimited {
            retry_after_seconds,
        } => PublicApiError::RateLimited {
            retry_after_seconds,
        },
        PublicRefreshError::Unavailable => PublicApiError::Unavailable,
    }
}

pub enum PublicApiError {
    Validation,
    NotFound,
    RefreshUnavailable,
    RateLimited { retry_after_seconds: u64 },
    Unavailable,
}
impl IntoResponse for PublicApiError {
    fn into_response(self) -> Response {
        let (status, code, message, retry_after_seconds) = match self {
            Self::Validation => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                "request input is invalid",
                None,
            ),
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                "resource_not_found",
                "the public resource was not found",
                None,
            ),
            Self::RefreshUnavailable => (
                StatusCode::CONFLICT,
                "refresh_unavailable",
                "public refresh is not available for this domain",
                None,
            ),
            Self::RateLimited {
                retry_after_seconds,
            } => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "the public refresh request rate limit has been exceeded",
                Some(retry_after_seconds),
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                "the public read service is unavailable",
                None,
            ),
        };
        let mut response = (
            status,
            Json(ErrorEnvelope {
                error: ErrorBody { code, message },
            }),
        )
            .into_response();
        if let Some(retry_after_seconds) = retry_after_seconds
            && let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}
