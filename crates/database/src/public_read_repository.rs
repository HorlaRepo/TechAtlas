use async_trait::async_trait;
use serde_json::Value;
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use techatlas_models::{
    CanonicalDomain, PublicAdoption, PublicAdoptionHistory, PublicAdoptionHistoryStatus,
    PublicAdoptionPoint, PublicAdoptionSeries, PublicAnalyticsDiscovery, PublicAnalyticsDomain,
    PublicAnalyticsMovers, PublicAnalyticsOverview, PublicAnalyticsProviderMover,
    PublicAnalyticsRank, PublicAnalyticsRankings, PublicAnalyticsTechnologyMover, PublicChange,
    PublicChangeTrend, PublicComparisonCell, PublicCountryRank, PublicCrawl, PublicCrawlDetail,
    PublicDnsObservation, PublicDomainProfile, PublicDomainSummary, PublicEvidence,
    PublicFrequentCrawlDomain, PublicPage, PublicProviderDomainPage, PublicProviderProfile,
    PublicReadError, PublicReadOperations, PublicRedirect, PublicRelatedTechnology,
    PublicTechnology, PublicTechnologyCategory, PublicTechnologyDomainPage,
    PublicTechnologyHistoryPoint, PublicTechnologyLibraryItem, PublicTechnologyLibraryPage,
    PublicTechnologyMigration, PublicTechnologyProfile, PublicTechnologyTrend,
    PublicTlsObservation,
};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

pub struct PostgresPublicReadRepository {
    pool: PgPool,
}

impl PostgresPublicReadRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn domain_id(&self, domain: &CanonicalDomain) -> Result<Option<Uuid>, PublicReadError> {
        sqlx::query_scalar(
            "SELECT id FROM domains WHERE canonical_domain = $1 AND archived_at IS NULL",
        )
        .bind(domain.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)
    }

    async fn technology_movers(
        &self,
        since: OffsetDateTime,
        limit: usize,
        growing: bool,
    ) -> Result<Vec<PublicAnalyticsTechnologyMover>, PublicReadError> {
        let direction = if growing { "DESC" } else { "ASC" };
        let comparison = if growing { ">" } else { "<" };
        let query = format!(
            "WITH change_deltas AS ( \
                SELECT to_technology_id AS technology_id, 1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('added', 'migrated') AND to_technology_id IS NOT NULL \
                UNION ALL \
                SELECT from_technology_id AS technology_id, -1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('removed', 'migrated') AND from_technology_id IS NOT NULL \
             ), trend_scores AS (SELECT technology_id, SUM(delta)::BIGINT AS net_change FROM change_deltas GROUP BY technology_id) \
             SELECT technologies.slug, technologies.display_name, technology_categories.slug AS category_slug, \
                technology_categories.display_name AS category_name, COUNT(domain_current_technologies.domain_id)::BIGINT AS adoption_count, \
                trend_scores.net_change \
             FROM trend_scores \
             JOIN technologies ON technologies.id = trend_scores.technology_id \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             LEFT JOIN domain_current_technologies ON domain_current_technologies.technology_id = technologies.id \
             WHERE trend_scores.net_change {comparison} 0 \
             GROUP BY technologies.id, technology_categories.id, trend_scores.net_change \
             ORDER BY trend_scores.net_change {direction}, technologies.slug LIMIT $2"
        );
        sqlx::query(&query)
            .bind(since)
            .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| PublicReadError::Unavailable)?
            .into_iter()
            .map(technology_mover_from_row)
            .collect()
    }

    async fn provider_movers(
        &self,
        since: OffsetDateTime,
        limit: usize,
        growing: bool,
    ) -> Result<Vec<PublicAnalyticsProviderMover>, PublicReadError> {
        let direction = if growing { "DESC" } else { "ASC" };
        let comparison = if growing { ">" } else { "<" };
        let query = format!(
            "WITH change_deltas AS ( \
                SELECT to_technology_id AS technology_id, 1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('added', 'migrated') AND to_technology_id IS NOT NULL \
                UNION ALL \
                SELECT from_technology_id AS technology_id, -1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('removed', 'migrated') AND from_technology_id IS NOT NULL \
             ), provider_scores AS ( \
                SELECT technology_provider_mappings.provider_id, SUM(change_deltas.delta)::BIGINT AS net_change \
                FROM change_deltas \
                JOIN technology_provider_mappings ON technology_provider_mappings.technology_id = change_deltas.technology_id \
                GROUP BY technology_provider_mappings.provider_id \
             ) \
             SELECT providers.slug, providers.display_name, \
                COUNT(DISTINCT domain_current_technologies.domain_id)::BIGINT AS adoption_count, provider_scores.net_change \
             FROM provider_scores \
             JOIN providers ON providers.id = provider_scores.provider_id \
             LEFT JOIN technology_provider_mappings ON technology_provider_mappings.provider_id = providers.id \
             LEFT JOIN domain_current_technologies \
                ON domain_current_technologies.technology_id = technology_provider_mappings.technology_id \
             WHERE provider_scores.net_change {comparison} 0 \
             GROUP BY providers.id, provider_scores.net_change \
             ORDER BY provider_scores.net_change {direction}, providers.slug LIMIT $2"
        );
        sqlx::query(&query)
            .bind(since)
            .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| PublicReadError::Unavailable)?
            .into_iter()
            .map(provider_mover_from_row)
            .collect()
    }
}

#[async_trait]
impl PublicReadOperations for PostgresPublicReadRepository {
    async fn domain_profile(
        &self,
        domain: &CanonicalDomain,
    ) -> Result<Option<PublicDomainProfile>, PublicReadError> {
        let row = sqlx::query(
            "SELECT domains.id, domains.canonical_domain, domains.created_at, crawl_policies.last_successful_crawl_at, country.country_code \
             FROM domains JOIN crawl_policies ON crawl_policies.domain_id = domains.id \
             LEFT JOIN LATERAL (SELECT country_code FROM crawl_country_observations \
                 JOIN crawl_snapshots ON crawl_snapshots.id = crawl_country_observations.crawl_snapshot_id \
                 WHERE crawl_snapshots.domain_id = domains.id ORDER BY crawl_snapshots.captured_at DESC LIMIT 1) country ON TRUE \
             WHERE domains.canonical_domain = $1 AND domains.archived_at IS NULL")
            .bind(domain.as_str()).fetch_optional(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let domain_id: Uuid = row
            .try_get("id")
            .map_err(|_| PublicReadError::Unavailable)?;
        let technology_rows = sqlx::query(
            "SELECT technologies.slug, technologies.display_name, technology_categories.slug AS category_slug, \
                technology_categories.display_name AS category_name, detections.confidence, detections.method, \
                detection_rule_versions.version AS rule_version, domain_current_technologies.first_observed_at, \
                domain_current_technologies.last_observed_at, detections.id AS detection_id \
             FROM domain_current_technologies JOIN technologies ON technologies.id = domain_current_technologies.technology_id \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             JOIN detections ON detections.id = domain_current_technologies.current_detection_id \
             JOIN detection_rule_versions ON detection_rule_versions.id = detections.detection_rule_version_id \
             WHERE domain_current_technologies.domain_id = $1 ORDER BY technology_categories.slug, technologies.slug")
            .bind(domain_id).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let mut technologies = Vec::with_capacity(technology_rows.len());
        for technology in technology_rows {
            let detection_id: Uuid = technology
                .try_get("detection_id")
                .map_err(|_| PublicReadError::Unavailable)?;
            let evidence_rows = sqlx::query("SELECT source, evidence_key, evidence_value FROM detection_evidence WHERE detection_id = $1 ORDER BY source, evidence_key, evidence_value")
                .bind(detection_id).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
            let evidence = evidence_rows
                .into_iter()
                .map(|e| {
                    Ok(PublicEvidence {
                        source: e
                            .try_get("source")
                            .map_err(|_| PublicReadError::Unavailable)?,
                        key: e
                            .try_get("evidence_key")
                            .map_err(|_| PublicReadError::Unavailable)?,
                        value: e
                            .try_get("evidence_value")
                            .map_err(|_| PublicReadError::Unavailable)?,
                    })
                })
                .collect::<Result<Vec<_>, PublicReadError>>()?;
            technologies.push(PublicTechnology {
                slug: technology
                    .try_get("slug")
                    .map_err(|_| PublicReadError::Unavailable)?,
                display_name: technology
                    .try_get("display_name")
                    .map_err(|_| PublicReadError::Unavailable)?,
                category_slug: technology
                    .try_get("category_slug")
                    .map_err(|_| PublicReadError::Unavailable)?,
                category_name: technology
                    .try_get("category_name")
                    .map_err(|_| PublicReadError::Unavailable)?,
                confidence: u8::try_from(
                    technology
                        .try_get::<i16, _>("confidence")
                        .map_err(|_| PublicReadError::Unavailable)?,
                )
                .map_err(|_| PublicReadError::Unavailable)?,
                method: technology
                    .try_get("method")
                    .map_err(|_| PublicReadError::Unavailable)?,
                rule_version: u16::try_from(
                    technology
                        .try_get::<i16, _>("rule_version")
                        .map_err(|_| PublicReadError::Unavailable)?,
                )
                .map_err(|_| PublicReadError::Unavailable)?,
                first_observed_at: technology
                    .try_get("first_observed_at")
                    .map_err(|_| PublicReadError::Unavailable)?,
                last_observed_at: technology
                    .try_get("last_observed_at")
                    .map_err(|_| PublicReadError::Unavailable)?,
                evidence,
            });
        }
        Ok(Some(PublicDomainProfile {
            canonical_domain: row
                .try_get("canonical_domain")
                .map_err(|_| PublicReadError::Unavailable)?,
            first_indexed_at: row
                .try_get("created_at")
                .map_err(|_| PublicReadError::Unavailable)?,
            last_crawled_at: row
                .try_get("last_successful_crawl_at")
                .map_err(|_| PublicReadError::Unavailable)?,
            country_code: row
                .try_get("country_code")
                .map_err(|_| PublicReadError::Unavailable)?,
            technologies,
        }))
    }

    async fn domain_changes(
        &self,
        domain: &CanonicalDomain,
        cursor: Option<OffsetDateTime>,
        limit: usize,
    ) -> Result<PublicPage<PublicChange>, PublicReadError> {
        let Some(domain_id) = self.domain_id(domain).await? else {
            return Ok(PublicPage {
                items: vec![],
                next_cursor: None,
            });
        };
        let rows = sqlx::query("SELECT * FROM ( \
                SELECT technology_changes.change_kind, technology_categories.slug AS category_slug, from_tech.slug AS from_slug, to_tech.slug AS to_slug, \
                       NULL::TEXT AS technology_slug, NULL::TEXT AS from_version, NULL::TEXT AS to_version, NULL::UUID AS reprocessing_run_id, technology_changes.observed_at \
                FROM technology_changes JOIN technology_categories ON technology_categories.id = technology_changes.category_id \
                LEFT JOIN technologies from_tech ON from_tech.id = technology_changes.from_technology_id \
                LEFT JOIN technologies to_tech ON to_tech.id = technology_changes.to_technology_id WHERE technology_changes.domain_id = $1 \
                UNION ALL \
                SELECT 'version_changed' AS change_kind, categories.slug AS category_slug, technologies.slug AS from_slug, technologies.slug AS to_slug, \
                       technologies.slug AS technology_slug, version_changes.from_version, version_changes.to_version, version_changes.reprocessing_run_id, version_changes.observed_at \
                FROM technology_version_changes AS version_changes \
                JOIN technologies ON technologies.id = version_changes.technology_id \
                JOIN technology_categories AS categories ON categories.id = technologies.category_id WHERE version_changes.domain_id = $1 \
             ) AS changes WHERE ($2::timestamptz IS NULL OR changes.observed_at < $2) \
             ORDER BY changes.observed_at DESC LIMIT $3")
            .bind(domain_id).bind(cursor).bind(i64::try_from(limit.saturating_add(1)).map_err(|_| PublicReadError::Unavailable)?).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let mut items = rows
            .into_iter()
            .map(|r| {
                Ok(PublicChange {
                    kind: r
                        .try_get("change_kind")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    category_slug: r
                        .try_get("category_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    from_technology_slug: r
                        .try_get("from_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    to_technology_slug: r
                        .try_get("to_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    technology_slug: r
                        .try_get("technology_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    from_version: r
                        .try_get("from_version")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    to_version: r
                        .try_get("to_version")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    reprocessing_run_id: r
                        .try_get::<Option<uuid::Uuid>, _>("reprocessing_run_id")
                        .map_err(|_| PublicReadError::Unavailable)?
                        .map(|value| value.to_string()),
                    observed_at: r
                        .try_get("observed_at")
                        .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, PublicReadError>>()?;
        let next_cursor = (items.len() > limit)
            .then(|| items.pop().map(|value| value.observed_at.to_string()))
            .flatten();
        Ok(PublicPage { items, next_cursor })
    }

    async fn domain_crawls(
        &self,
        domain: &CanonicalDomain,
        cursor: Option<OffsetDateTime>,
        limit: usize,
    ) -> Result<PublicPage<PublicCrawl>, PublicReadError> {
        let Some(domain_id) = self.domain_id(domain).await? else {
            return Ok(PublicPage {
                items: vec![],
                next_cursor: None,
            });
        };
        let rows = sqlx::query("SELECT id, requested_url, final_url, response_status, captured_at FROM crawl_snapshots WHERE domain_id = $1 AND ($2::timestamptz IS NULL OR captured_at < $2) ORDER BY captured_at DESC LIMIT $3")
            .bind(domain_id).bind(cursor).bind(i64::try_from(limit.saturating_add(1)).map_err(|_| PublicReadError::Unavailable)?).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let mut items = rows
            .into_iter()
            .map(crawl_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = (items.len() > limit)
            .then(|| items.pop().map(|value| value.captured_at.to_string()))
            .flatten();
        Ok(PublicPage { items, next_cursor })
    }

    async fn crawl_detail(
        &self,
        domain: &CanonicalDomain,
        crawl_id: &str,
    ) -> Result<Option<PublicCrawlDetail>, PublicReadError> {
        let Some(domain_id) = self.domain_id(domain).await? else {
            return Ok(None);
        };
        let crawl_id = Uuid::parse_str(crawl_id).map_err(|_| PublicReadError::Unavailable)?;
        let row = sqlx::query("SELECT crawl_snapshots.id, crawl_snapshots.requested_url, crawl_snapshots.final_url, crawl_snapshots.response_status, crawl_snapshots.captured_at, crawl_snapshots.response_headers, crawl_snapshots.redirect_chain, dns.source AS dns_source, dns.observed_at AS dns_observed_at, dns.queried_name AS dns_queried_name, dns.addresses AS dns_addresses, dns.unavailable_reason AS dns_unavailable_reason, tls.source AS tls_source, tls.observed_at AS tls_observed_at, tls.unavailable_reason AS tls_unavailable_reason, tls.validation_status AS tls_validation_status, tls.protocol AS tls_protocol, tls.cipher_suite AS tls_cipher_suite, tls.certificate_subject AS tls_certificate_subject, tls.certificate_issuer AS tls_certificate_issuer, tls.subject_alternative_names AS tls_subject_alternative_names, tls.certificate_not_before AS tls_certificate_not_before, tls.certificate_not_after AS tls_certificate_not_after FROM crawl_snapshots LEFT JOIN crawl_dns_observations AS dns ON dns.crawl_snapshot_id = crawl_snapshots.id LEFT JOIN crawl_tls_observations AS tls ON tls.crawl_snapshot_id = crawl_snapshots.id WHERE crawl_snapshots.id = $1 AND crawl_snapshots.domain_id = $2")
            .bind(crawl_id).bind(domain_id).fetch_optional(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let headers: Value = row
            .try_get("response_headers")
            .map_err(|_| PublicReadError::Unavailable)?;
        let redirects: Value = row
            .try_get("redirect_chain")
            .map_err(|_| PublicReadError::Unavailable)?;
        let headers = serde_json::from_value::<BTreeMap<String, String>>(headers)
            .map_err(|_| PublicReadError::Unavailable)?;
        let redirect_chain = redirects
            .as_array()
            .ok_or(PublicReadError::Unavailable)?
            .iter()
            .map(|value| {
                Ok(PublicRedirect {
                    from_url: value
                        .get("from_url")
                        .and_then(Value::as_str)
                        .ok_or(PublicReadError::Unavailable)?
                        .to_owned(),
                    to_url: value
                        .get("to_url")
                        .and_then(Value::as_str)
                        .ok_or(PublicReadError::Unavailable)?
                        .to_owned(),
                    status: u16::try_from(
                        value
                            .get("status")
                            .and_then(Value::as_u64)
                            .ok_or(PublicReadError::Unavailable)?,
                    )
                    .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let country_code = sqlx::query_scalar(
            "SELECT country_code FROM crawl_country_observations WHERE crawl_snapshot_id = $1",
        )
        .bind(crawl_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let captured_at: OffsetDateTime = row
            .try_get("captured_at")
            .map_err(|_| PublicReadError::Unavailable)?;
        let dns = dns_from_row(&row, captured_at)?;
        let tls = tls_from_row(&row, captured_at)?;
        let crawl = crawl_from_row(row)?;
        Ok(Some(PublicCrawlDetail {
            crawl,
            response_headers: headers,
            redirect_chain,
            country_code,
            dns,
            tls,
        }))
    }

    async fn technologies(
        &self,
        category: Option<&str>,
        trend: Option<PublicTechnologyTrend>,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<PublicTechnologyLibraryPage, PublicReadError> {
        let category_rows =
            sqlx::query("SELECT slug, display_name FROM technology_categories ORDER BY slug")
                .fetch_all(&self.pool)
                .await
                .map_err(|_| PublicReadError::Unavailable)?;
        let categories = category_rows
            .into_iter()
            .map(|row| {
                Ok(PublicTechnologyCategory {
                    slug: row
                        .try_get("slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    display_name: row
                        .try_get("display_name")
                        .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, PublicReadError>>()?;

        let rows = sqlx::query(
            "WITH change_deltas AS ( \
                SELECT to_technology_id AS technology_id, 1::BIGINT AS delta \
                FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('added', 'migrated') AND to_technology_id IS NOT NULL \
                UNION ALL \
                SELECT from_technology_id AS technology_id, -1::BIGINT AS delta \
                FROM technology_changes \
                WHERE observed_at >= $1 AND change_kind IN ('removed', 'migrated') AND from_technology_id IS NOT NULL \
             ), trend_scores AS ( \
                SELECT technology_id, SUM(delta)::BIGINT AS net_change \
                FROM change_deltas GROUP BY technology_id \
             ) \
             SELECT technologies.slug, technologies.display_name, technology_categories.slug AS category_slug, \
                technology_categories.display_name AS category_name, \
                COUNT(domain_current_technologies.domain_id)::BIGINT AS adoption_count, \
                COALESCE(trend_scores.net_change, 0)::BIGINT AS net_change \
             FROM technologies \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             LEFT JOIN domain_current_technologies ON domain_current_technologies.technology_id = technologies.id \
             LEFT JOIN trend_scores ON trend_scores.technology_id = technologies.id \
             WHERE ($2::text IS NULL OR technology_categories.slug = $2) \
                AND ($3::text IS NULL OR technologies.slug > $3) \
             GROUP BY technologies.id, technology_categories.id, trend_scores.net_change \
             HAVING ($4::text IS NULL \
                OR ($4 = 'growing' AND COALESCE(trend_scores.net_change, 0) > 0) \
                OR ($4 = 'declining' AND COALESCE(trend_scores.net_change, 0) < 0) \
                OR ($4 = 'stable' AND COALESCE(trend_scores.net_change, 0) = 0)) \
             ORDER BY technologies.slug LIMIT $5",
        )
        .bind(trend_since)
        .bind(category)
        .bind(cursor)
        .bind(trend.map(PublicTechnologyTrend::as_str))
        .bind(i64::try_from(limit.saturating_add(1)).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let mut items = rows
            .into_iter()
            .map(technology_library_item_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = (items.len() > limit)
            .then(|| items.pop().map(|value| value.slug))
            .flatten();
        Ok(PublicTechnologyLibraryPage {
            items,
            categories,
            next_cursor,
        })
    }

    async fn technology_domains(
        &self,
        technology: &str,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<Option<PublicPage<PublicDomainSummary>>, PublicReadError> {
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM technologies WHERE slug = $1")
                .bind(technology)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| PublicReadError::Unavailable)?;
        if exists.is_none() {
            return Ok(None);
        }
        let rows = sqlx::query("SELECT domains.canonical_domain, crawl_policies.last_successful_crawl_at FROM domain_current_technologies JOIN technologies ON technologies.id = domain_current_technologies.technology_id JOIN domains ON domains.id = domain_current_technologies.domain_id JOIN crawl_policies ON crawl_policies.domain_id = domains.id WHERE technologies.slug = $1 AND ($2::text IS NULL OR domains.canonical_domain > $2) ORDER BY domains.canonical_domain LIMIT $3")
            .bind(technology).bind(cursor).bind(i64::try_from(limit.saturating_add(1)).map_err(|_| PublicReadError::Unavailable)?).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        let mut items = rows
            .into_iter()
            .map(|r| {
                Ok(PublicDomainSummary {
                    canonical_domain: r
                        .try_get("canonical_domain")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    last_crawled_at: r
                        .try_get("last_successful_crawl_at")
                        .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, PublicReadError>>()?;
        let next_cursor = (items.len() > limit)
            .then(|| items.pop().map(|value| value.canonical_domain))
            .flatten();
        Ok(Some(PublicPage { items, next_cursor }))
    }

    async fn technology_profile(
        &self,
        technology: &str,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<Option<PublicTechnologyProfile>, PublicReadError> {
        let technology_row = sqlx::query(
            "SELECT technologies.id, technologies.slug, technologies.display_name, \
                technology_categories.slug AS category_slug, technology_categories.display_name AS category_name, \
                COUNT(domain_current_technologies.domain_id)::BIGINT AS adoption_count, \
                ((SELECT COUNT(*) FROM technology_changes \
                    WHERE observed_at >= $1 AND to_technology_id = technologies.id \
                        AND change_kind IN ('added', 'migrated')) \
                 - (SELECT COUNT(*) FROM technology_changes \
                    WHERE observed_at >= $1 AND from_technology_id = technologies.id \
                        AND change_kind IN ('removed', 'migrated')))::BIGINT AS net_change \
             FROM technologies \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             LEFT JOIN domain_current_technologies ON domain_current_technologies.technology_id = technologies.id \
             WHERE technologies.slug = $2 \
             GROUP BY technologies.id, technology_categories.id",
        )
        .bind(trend_since)
        .bind(technology)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let Some(technology_row) = technology_row else {
            return Ok(None);
        };
        let technology_id: Uuid = technology_row
            .try_get("id")
            .map_err(|_| PublicReadError::Unavailable)?;
        let net_change: i64 = technology_row
            .try_get("net_change")
            .map_err(|_| PublicReadError::Unavailable)?;

        let history_rows = sqlx::query(
            "SELECT to_char(date_trunc('day', observed_at), 'YYYY-MM-DD') AS day, SUM(delta)::BIGINT AS net_change \
             FROM ( \
                SELECT observed_at, 1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND to_technology_id = $2 AND change_kind IN ('added', 'migrated') \
                UNION ALL \
                SELECT observed_at, -1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $1 AND from_technology_id = $2 AND change_kind IN ('removed', 'migrated') \
             ) AS deltas \
             GROUP BY date_trunc('day', observed_at) \
             ORDER BY date_trunc('day', observed_at)",
        )
        .bind(trend_since)
        .bind(technology_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let history = history_rows
            .into_iter()
            .map(|row| {
                Ok(PublicTechnologyHistoryPoint {
                    day: row
                        .try_get("day")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    net_change: row
                        .try_get("net_change")
                        .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, PublicReadError>>()?;

        let related_rows = sqlx::query(
            "SELECT related.slug, related.display_name, categories.slug AS category_slug, \
                categories.display_name AS category_name, COUNT(DISTINCT other_current.domain_id)::BIGINT AS shared_domain_count \
             FROM domain_current_technologies selected_current \
             JOIN domain_current_technologies other_current \
                ON other_current.domain_id = selected_current.domain_id AND other_current.technology_id <> $1 \
             JOIN technologies related ON related.id = other_current.technology_id \
             JOIN technology_categories categories ON categories.id = related.category_id \
             WHERE selected_current.technology_id = $1 \
             GROUP BY related.id, categories.id \
             ORDER BY shared_domain_count DESC, related.slug \
             LIMIT 6",
        )
        .bind(technology_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let related_technologies = related_rows
            .into_iter()
            .map(|row| {
                Ok(PublicRelatedTechnology {
                    slug: row
                        .try_get("slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    display_name: row
                        .try_get("display_name")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    category_slug: row
                        .try_get("category_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    category_name: row
                        .try_get("category_name")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    shared_domain_count: u64::try_from(
                        row.try_get::<i64, _>("shared_domain_count")
                            .map_err(|_| PublicReadError::Unavailable)?,
                    )
                    .map_err(|_| PublicReadError::Unavailable)?,
                })
            })
            .collect::<Result<Vec<_>, PublicReadError>>()?;

        let domains = self
            .technology_domains(technology, cursor, limit)
            .await?
            .ok_or(PublicReadError::Unavailable)?;
        Ok(Some(PublicTechnologyProfile {
            slug: technology_row
                .try_get("slug")
                .map_err(|_| PublicReadError::Unavailable)?,
            display_name: technology_row
                .try_get("display_name")
                .map_err(|_| PublicReadError::Unavailable)?,
            category_slug: technology_row
                .try_get("category_slug")
                .map_err(|_| PublicReadError::Unavailable)?,
            category_name: technology_row
                .try_get("category_name")
                .map_err(|_| PublicReadError::Unavailable)?,
            adoption_count: u64::try_from(
                technology_row
                    .try_get::<i64, _>("adoption_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?,
            net_change,
            trend: PublicTechnologyTrend::from_net_change(net_change),
            history,
            related_technologies,
            domains: PublicTechnologyDomainPage {
                items: domains.items,
                next_cursor: domains.next_cursor,
            },
        }))
    }

    async fn comparison(
        &self,
        domains: &[CanonicalDomain],
        stale_before: OffsetDateTime,
    ) -> Result<Vec<PublicComparisonCell>, PublicReadError> {
        let names = domains
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>();
        let rows = sqlx::query("WITH requested AS (SELECT unnest($1::text[]) AS canonical_domain), categories AS (SELECT slug FROM technology_categories) SELECT requested.canonical_domain, categories.slug AS category_slug, technologies.slug AS technology_slug, domain_current_technologies.last_observed_at, crawl_policies.last_successful_crawl_at FROM requested CROSS JOIN categories LEFT JOIN domains ON domains.canonical_domain = requested.canonical_domain AND domains.archived_at IS NULL LEFT JOIN crawl_policies ON crawl_policies.domain_id = domains.id LEFT JOIN domain_current_technologies ON domain_current_technologies.domain_id = domains.id LEFT JOIN technologies ON technologies.id = domain_current_technologies.technology_id AND technologies.category_id = (SELECT id FROM technology_categories WHERE slug = categories.slug) ORDER BY categories.slug, requested.canonical_domain, technologies.slug")
            .bind(names).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        rows.into_iter()
            .map(|r| {
                let observed: Option<OffsetDateTime> = r
                    .try_get("last_observed_at")
                    .map_err(|_| PublicReadError::Unavailable)?;
                let crawl: Option<OffsetDateTime> = r
                    .try_get("last_successful_crawl_at")
                    .map_err(|_| PublicReadError::Unavailable)?;
                let state = if observed.is_none() {
                    "unknown"
                } else if crawl.is_none_or(|at| at < stale_before) {
                    "stale"
                } else {
                    "current"
                };
                Ok(PublicComparisonCell {
                    category_slug: r
                        .try_get("category_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    canonical_domain: r
                        .try_get("canonical_domain")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    technology_slug: r
                        .try_get("technology_slug")
                        .map_err(|_| PublicReadError::Unavailable)?,
                    state: state.to_owned(),
                    last_observed_at: observed,
                })
            })
            .collect()
    }

    async fn analytics_overview(&self) -> Result<PublicAnalyticsOverview, PublicReadError> {
        let row = sqlx::query("SELECT (SELECT COUNT(*) FROM domains WHERE archived_at IS NULL) AS domain_count, (SELECT COUNT(*) FROM technologies) AS technology_count, (SELECT COUNT(*) FROM domain_current_technologies) AS current_detection_count, (SELECT COUNT(DISTINCT country_code) FROM crawl_country_observations) AS country_count").fetch_one(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?;
        Ok(PublicAnalyticsOverview {
            domain_count: u64::try_from(
                row.try_get::<i64, _>("domain_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?,
            technology_count: u64::try_from(
                row.try_get::<i64, _>("technology_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?,
            current_detection_count: u64::try_from(
                row.try_get::<i64, _>("current_detection_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?,
            country_count: u64::try_from(
                row.try_get::<i64, _>("country_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?,
        })
    }

    async fn adoption(&self) -> Result<Vec<PublicAdoption>, PublicReadError> {
        sqlx::query("SELECT technologies.slug, COUNT(domain_current_technologies.domain_id) AS count FROM technologies LEFT JOIN domain_current_technologies ON domain_current_technologies.technology_id = technologies.id GROUP BY technologies.slug ORDER BY count DESC, technologies.slug").fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?.into_iter().map(|r| Ok(PublicAdoption { slug: r.try_get("slug").map_err(|_| PublicReadError::Unavailable)?, count: u64::try_from(r.try_get::<i64, _>("count").map_err(|_| PublicReadError::Unavailable)?).map_err(|_| PublicReadError::Unavailable)? })).collect()
    }

    async fn adoption_history(
        &self,
        since: Date,
        technology: Option<&str>,
    ) -> Result<PublicAdoptionHistory, PublicReadError> {
        let availability = sqlx::query(
            "SELECT TO_CHAR(MIN(observed_on), 'YYYY-MM-DD') AS available_from, \
                    TO_CHAR(MAX(observed_on), 'YYYY-MM-DD') AS available_to, \
                    COUNT(DISTINCT observed_on)::BIGINT AS day_count \
             FROM technology_adoption_daily WHERE observed_on >= $1",
        )
        .bind(since)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let day_count = u64::try_from(
            availability
                .try_get::<i64, _>("day_count")
                .map_err(|_| PublicReadError::Unavailable)?,
        )
        .map_err(|_| PublicReadError::Unavailable)?;
        let available_from = availability
            .try_get("available_from")
            .map_err(|_| PublicReadError::Unavailable)?;
        let available_to = availability
            .try_get("available_to")
            .map_err(|_| PublicReadError::Unavailable)?;
        let rows = sqlx::query(
            "WITH latest_day AS ( \
                SELECT MAX(observed_on) AS observed_on FROM technology_adoption_daily WHERE observed_on >= $1 \
             ), top_technologies AS ( \
                SELECT technology_adoption_daily.technology_id, \
                       ROW_NUMBER() OVER (ORDER BY technology_adoption_daily.domain_count DESC, technology_adoption_daily.technology_id) AS priority \
                FROM technology_adoption_daily \
                JOIN latest_day ON technology_adoption_daily.observed_on = latest_day.observed_on \
                ORDER BY technology_adoption_daily.domain_count DESC, technology_adoption_daily.technology_id \
                LIMIT 5 \
             ), requested_technology AS ( \
                SELECT id AS technology_id, 6::BIGINT AS priority FROM technologies WHERE slug = $2 \
             ), chosen_technologies AS ( \
                SELECT technology_id, priority FROM top_technologies \
                UNION ALL \
                SELECT requested_technology.technology_id, requested_technology.priority \
                FROM requested_technology \
                WHERE NOT EXISTS ( \
                    SELECT 1 FROM top_technologies \
                    WHERE top_technologies.technology_id = requested_technology.technology_id \
                ) \
             ) \
             SELECT technologies.slug, technologies.display_name, \
                    TO_CHAR(technology_adoption_daily.observed_on, 'YYYY-MM-DD') AS day, \
                    technology_adoption_daily.domain_count \
             FROM technology_adoption_daily \
             JOIN chosen_technologies ON chosen_technologies.technology_id = technology_adoption_daily.technology_id \
             JOIN technologies ON technologies.id = technology_adoption_daily.technology_id \
             WHERE technology_adoption_daily.observed_on >= $1 \
             ORDER BY chosen_technologies.priority, technology_adoption_daily.observed_on",
        )
        .bind(since)
        .bind(technology)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let mut series = Vec::<PublicAdoptionSeries>::new();
        for row in rows {
            let slug: String = row
                .try_get("slug")
                .map_err(|_| PublicReadError::Unavailable)?;
            let display_name: String = row
                .try_get("display_name")
                .map_err(|_| PublicReadError::Unavailable)?;
            let count = u64::try_from(
                row.try_get::<i64, _>("domain_count")
                    .map_err(|_| PublicReadError::Unavailable)?,
            )
            .map_err(|_| PublicReadError::Unavailable)?;
            let point = PublicAdoptionPoint {
                day: row
                    .try_get("day")
                    .map_err(|_| PublicReadError::Unavailable)?,
                count,
            };
            if let Some(current) = series.last_mut().filter(|current| current.slug == slug) {
                current.points.push(point);
            } else {
                series.push(PublicAdoptionSeries {
                    slug,
                    display_name,
                    points: vec![point],
                });
            }
        }
        Ok(PublicAdoptionHistory {
            status: if day_count >= 2 {
                PublicAdoptionHistoryStatus::Ready
            } else {
                PublicAdoptionHistoryStatus::InsufficientHistory
            },
            available_from,
            available_to,
            series,
        })
    }

    async fn analytics_rankings(
        &self,
        limit: usize,
    ) -> Result<PublicAnalyticsRankings, PublicReadError> {
        let technology_rows = sqlx::query(
            "SELECT technologies.slug, technologies.display_name, COUNT(domain_current_technologies.domain_id)::BIGINT AS count \
             FROM technologies LEFT JOIN domain_current_technologies \
                ON domain_current_technologies.technology_id = technologies.id \
             GROUP BY technologies.id ORDER BY count DESC, technologies.slug LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let category_rows = sqlx::query(
            "SELECT technology_categories.slug, technology_categories.display_name, \
                COUNT(DISTINCT domain_current_technologies.domain_id)::BIGINT AS count \
             FROM technology_categories LEFT JOIN technologies \
                ON technologies.category_id = technology_categories.id \
             LEFT JOIN domain_current_technologies \
                ON domain_current_technologies.technology_id = technologies.id \
             GROUP BY technology_categories.id \
             ORDER BY count DESC, technology_categories.slug LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let country_rows = sqlx::query(
            "WITH latest_country AS ( \
                SELECT DISTINCT ON (crawl_snapshots.domain_id) crawl_snapshots.domain_id, crawl_country_observations.country_code \
                FROM crawl_country_observations \
                JOIN crawl_snapshots ON crawl_snapshots.id = crawl_country_observations.crawl_snapshot_id \
                JOIN domains ON domains.id = crawl_snapshots.domain_id AND domains.archived_at IS NULL \
                ORDER BY crawl_snapshots.domain_id, crawl_snapshots.captured_at DESC \
             ) \
             SELECT country_code, COUNT(*)::BIGINT AS count FROM latest_country \
             GROUP BY country_code ORDER BY count DESC, country_code LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let provider_rows = sqlx::query(
            "SELECT providers.slug, providers.display_name, \
                COUNT(DISTINCT domain_current_technologies.domain_id)::BIGINT AS count \
             FROM providers \
             LEFT JOIN technology_provider_mappings ON technology_provider_mappings.provider_id = providers.id \
             LEFT JOIN domain_current_technologies \
                ON domain_current_technologies.technology_id = technology_provider_mappings.technology_id \
             GROUP BY providers.id ORDER BY count DESC, providers.slug LIMIT $1",
        )
        .bind(i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;

        Ok(PublicAnalyticsRankings {
            technologies: technology_rows
                .into_iter()
                .map(analytics_rank_from_row)
                .collect::<Result<Vec<_>, _>>()?,
            categories: category_rows
                .into_iter()
                .map(analytics_rank_from_row)
                .collect::<Result<Vec<_>, _>>()?,
            countries: country_rows
                .into_iter()
                .map(country_rank_from_row)
                .collect::<Result<Vec<_>, _>>()?,
            providers: provider_rows
                .into_iter()
                .map(analytics_rank_from_row)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    async fn analytics_movers(
        &self,
        since: OffsetDateTime,
        limit: usize,
    ) -> Result<PublicAnalyticsMovers, PublicReadError> {
        let growing_technologies = self.technology_movers(since, limit, true).await?;
        let declining_technologies = self.technology_movers(since, limit, false).await?;
        let growing_providers = self.provider_movers(since, limit, true).await?;
        let declining_providers = self.provider_movers(since, limit, false).await?;
        Ok(PublicAnalyticsMovers {
            growing_technologies,
            declining_technologies,
            growing_providers,
            declining_providers,
        })
    }

    async fn change_trends(
        &self,
        since: OffsetDateTime,
    ) -> Result<Vec<PublicChangeTrend>, PublicReadError> {
        sqlx::query("SELECT to_char(date_trunc('day', observed_at), 'YYYY-MM-DD') AS day, COUNT(*) FILTER (WHERE change_kind = 'added') AS added, COUNT(*) FILTER (WHERE change_kind = 'removed') AS removed, COUNT(*) FILTER (WHERE change_kind = 'migrated') AS migrated FROM technology_changes WHERE observed_at >= $1 GROUP BY date_trunc('day', observed_at) ORDER BY date_trunc('day', observed_at)").bind(since).fetch_all(&self.pool).await.map_err(|_| PublicReadError::Unavailable)?.into_iter().map(|r| Ok(PublicChangeTrend { day: r.try_get("day").map_err(|_| PublicReadError::Unavailable)?, added: u64::try_from(r.try_get::<i64, _>("added").map_err(|_| PublicReadError::Unavailable)?).map_err(|_| PublicReadError::Unavailable)?, removed: u64::try_from(r.try_get::<i64, _>("removed").map_err(|_| PublicReadError::Unavailable)?).map_err(|_| PublicReadError::Unavailable)?, migrated: u64::try_from(r.try_get::<i64, _>("migrated").map_err(|_| PublicReadError::Unavailable)?).map_err(|_| PublicReadError::Unavailable)? })).collect()
    }

    async fn analytics_discovery(
        &self,
        since: OffsetDateTime,
        limit: usize,
    ) -> Result<PublicAnalyticsDiscovery, PublicReadError> {
        let limit = i64::try_from(limit).map_err(|_| PublicReadError::Unavailable)?;
        let migrations = sqlx::query(
            "SELECT from_technology.slug AS from_slug, from_technology.display_name AS from_display_name, \
                    to_technology.slug AS to_slug, to_technology.display_name AS to_display_name, \
                    COUNT(DISTINCT technology_changes.domain_id)::BIGINT AS domain_count \
             FROM technology_changes \
             JOIN technologies AS from_technology ON from_technology.id = technology_changes.from_technology_id \
             JOIN technologies AS to_technology ON to_technology.id = technology_changes.to_technology_id \
             WHERE technology_changes.change_kind = 'migrated' AND technology_changes.observed_at >= $1 \
             GROUP BY from_technology.id, to_technology.id \
             ORDER BY domain_count DESC, from_technology.slug, to_technology.slug LIMIT $2",
        )
        .bind(since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?
        .into_iter()
        .map(|row| {
            Ok(PublicTechnologyMigration {
                from_slug: row.try_get("from_slug").map_err(|_| PublicReadError::Unavailable)?,
                from_display_name: row.try_get("from_display_name").map_err(|_| PublicReadError::Unavailable)?,
                to_slug: row.try_get("to_slug").map_err(|_| PublicReadError::Unavailable)?,
                to_display_name: row.try_get("to_display_name").map_err(|_| PublicReadError::Unavailable)?,
                domain_count: count_from_row(&row, "domain_count")?,
            })
        })
        .collect::<Result<Vec<_>, PublicReadError>>()?;
        let newest_domains = sqlx::query(
            "SELECT domains.canonical_domain, MIN(crawl_snapshots.captured_at) AS first_indexed_at \
             FROM domains JOIN crawl_snapshots ON crawl_snapshots.domain_id = domains.id \
             WHERE domains.archived_at IS NULL \
             GROUP BY domains.id ORDER BY first_indexed_at DESC, domains.canonical_domain LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?
        .into_iter()
        .map(|row| {
            Ok(PublicAnalyticsDomain {
                canonical_domain: row
                    .try_get("canonical_domain")
                    .map_err(|_| PublicReadError::Unavailable)?,
                first_indexed_at: row
                    .try_get("first_indexed_at")
                    .map_err(|_| PublicReadError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>, PublicReadError>>()?;
        let frequently_crawled_domains = sqlx::query(
            "SELECT domains.canonical_domain, COUNT(crawl_snapshots.id)::BIGINT AS crawl_count, \
                    MAX(crawl_snapshots.captured_at) AS last_crawled_at \
             FROM domains JOIN crawl_snapshots ON crawl_snapshots.domain_id = domains.id \
             WHERE domains.archived_at IS NULL AND crawl_snapshots.captured_at >= $1 \
             GROUP BY domains.id ORDER BY crawl_count DESC, last_crawled_at DESC, domains.canonical_domain LIMIT $2",
        )
        .bind(since)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?
        .into_iter()
        .map(|row| {
            Ok(PublicFrequentCrawlDomain {
                canonical_domain: row.try_get("canonical_domain").map_err(|_| PublicReadError::Unavailable)?,
                crawl_count: count_from_row(&row, "crawl_count")?,
                last_crawled_at: row.try_get("last_crawled_at").map_err(|_| PublicReadError::Unavailable)?,
            })
        })
        .collect::<Result<Vec<_>, PublicReadError>>()?;
        Ok(PublicAnalyticsDiscovery {
            large_migrations: migrations,
            newest_domains,
            frequently_crawled_domains,
        })
    }

    async fn provider_profile(
        &self,
        provider: &str,
        cursor: Option<&str>,
        limit: usize,
        trend_since: OffsetDateTime,
    ) -> Result<Option<PublicProviderProfile>, PublicReadError> {
        let provider_row =
            sqlx::query("SELECT id, slug, display_name FROM providers WHERE slug = $1")
                .bind(provider)
                .fetch_optional(&self.pool)
                .await
                .map_err(|_| PublicReadError::Unavailable)?;
        let Some(provider_row) = provider_row else {
            return Ok(None);
        };
        let provider_id: Uuid = provider_row
            .try_get("id")
            .map_err(|_| PublicReadError::Unavailable)?;
        let slug = provider_row
            .try_get("slug")
            .map_err(|_| PublicReadError::Unavailable)?;
        let display_name = provider_row
            .try_get("display_name")
            .map_err(|_| PublicReadError::Unavailable)?;
        let stats = sqlx::query(
            "WITH change_deltas AS ( \
                SELECT to_technology_id AS technology_id, 1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $2 AND change_kind IN ('added', 'migrated') AND to_technology_id IS NOT NULL \
                UNION ALL \
                SELECT from_technology_id AS technology_id, -1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $2 AND change_kind IN ('removed', 'migrated') AND from_technology_id IS NOT NULL \
             ) \
             SELECT (SELECT COUNT(DISTINCT domain_current_technologies.domain_id)::BIGINT \
                FROM technology_provider_mappings \
                LEFT JOIN domain_current_technologies \
                    ON domain_current_technologies.technology_id = technology_provider_mappings.technology_id \
                WHERE technology_provider_mappings.provider_id = $1) AS adoption_count, \
                COALESCE((SELECT SUM(change_deltas.delta)::BIGINT FROM change_deltas \
                    JOIN technology_provider_mappings \
                        ON technology_provider_mappings.technology_id = change_deltas.technology_id \
                    WHERE technology_provider_mappings.provider_id = $1), 0)::BIGINT AS net_change",
        )
        .bind(provider_id)
        .bind(trend_since)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let adoption_count = count_from_row(&stats, "adoption_count")?;
        let net_change: i64 = stats
            .try_get("net_change")
            .map_err(|_| PublicReadError::Unavailable)?;
        let technology_rows = sqlx::query(
            "WITH change_deltas AS ( \
                SELECT to_technology_id AS technology_id, 1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $2 AND change_kind IN ('added', 'migrated') AND to_technology_id IS NOT NULL \
                UNION ALL \
                SELECT from_technology_id AS technology_id, -1::BIGINT AS delta FROM technology_changes \
                WHERE observed_at >= $2 AND change_kind IN ('removed', 'migrated') AND from_technology_id IS NOT NULL \
             ), trend_scores AS (SELECT technology_id, SUM(delta)::BIGINT AS net_change FROM change_deltas GROUP BY technology_id) \
             SELECT technologies.slug, technologies.display_name, technology_categories.slug AS category_slug, \
                technology_categories.display_name AS category_name, COUNT(domain_current_technologies.domain_id)::BIGINT AS adoption_count, \
                COALESCE(trend_scores.net_change, 0)::BIGINT AS net_change \
             FROM technology_provider_mappings \
             JOIN technologies ON technologies.id = technology_provider_mappings.technology_id \
             JOIN technology_categories ON technology_categories.id = technologies.category_id \
             LEFT JOIN domain_current_technologies ON domain_current_technologies.technology_id = technologies.id \
             LEFT JOIN trend_scores ON trend_scores.technology_id = technologies.id \
             WHERE technology_provider_mappings.provider_id = $1 \
             GROUP BY technologies.id, technology_categories.id, trend_scores.net_change \
             ORDER BY technologies.slug",
        )
        .bind(provider_id)
        .bind(trend_since)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let domain_rows = sqlx::query(
            "SELECT domains.canonical_domain, crawl_policies.last_successful_crawl_at \
             FROM domains \
             JOIN domain_current_technologies ON domain_current_technologies.domain_id = domains.id \
             JOIN technology_provider_mappings \
                ON technology_provider_mappings.technology_id = domain_current_technologies.technology_id \
             LEFT JOIN crawl_policies ON crawl_policies.domain_id = domains.id \
             WHERE technology_provider_mappings.provider_id = $1 AND domains.archived_at IS NULL \
                AND ($2::text IS NULL OR domains.canonical_domain > $2) \
             GROUP BY domains.id, crawl_policies.last_successful_crawl_at \
             ORDER BY domains.canonical_domain LIMIT $3",
        )
        .bind(provider_id)
        .bind(cursor)
        .bind(i64::try_from(limit.saturating_add(1)).map_err(|_| PublicReadError::Unavailable)?)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| PublicReadError::Unavailable)?;
        let mut domains = domain_rows
            .into_iter()
            .map(domain_summary_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = (domains.len() > limit)
            .then(|| domains.pop().map(|domain| domain.canonical_domain))
            .flatten();
        Ok(Some(PublicProviderProfile {
            slug,
            display_name,
            adoption_count,
            net_change,
            trend: PublicTechnologyTrend::from_net_change(net_change),
            technologies: technology_rows
                .into_iter()
                .map(technology_library_item_from_row)
                .collect::<Result<Vec<_>, _>>()?,
            domains: PublicProviderDomainPage {
                items: domains,
                next_cursor,
            },
        }))
    }
}

fn dns_from_row(
    row: &sqlx::postgres::PgRow,
    fallback_observed_at: OffsetDateTime,
) -> Result<PublicDnsObservation, PublicReadError> {
    let Some(source) = row
        .try_get::<Option<String>, _>("dns_source")
        .map_err(|_| PublicReadError::Unavailable)?
    else {
        return Ok(PublicDnsObservation {
            source: "not_captured".to_owned(),
            observed_at: fallback_observed_at,
            availability: "unavailable".to_owned(),
            unavailable_reason: Some("not_captured".to_owned()),
            queried_name: None,
            addresses: Vec::new(),
        });
    };
    let addresses = row
        .try_get::<Value, _>("dns_addresses")
        .map_err(|_| PublicReadError::Unavailable)?;
    Ok(PublicDnsObservation {
        source,
        observed_at: row
            .try_get("dns_observed_at")
            .map_err(|_| PublicReadError::Unavailable)?,
        availability: if row
            .try_get::<Option<String>, _>("dns_unavailable_reason")
            .map_err(|_| PublicReadError::Unavailable)?
            .is_some()
        {
            "unavailable".to_owned()
        } else {
            "available".to_owned()
        },
        unavailable_reason: row
            .try_get("dns_unavailable_reason")
            .map_err(|_| PublicReadError::Unavailable)?,
        queried_name: row
            .try_get("dns_queried_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        addresses: serde_json::from_value(addresses).map_err(|_| PublicReadError::Unavailable)?,
    })
}

fn tls_from_row(
    row: &sqlx::postgres::PgRow,
    fallback_observed_at: OffsetDateTime,
) -> Result<PublicTlsObservation, PublicReadError> {
    let Some(source) = row
        .try_get::<Option<String>, _>("tls_source")
        .map_err(|_| PublicReadError::Unavailable)?
    else {
        return Ok(PublicTlsObservation {
            source: "not_captured".to_owned(),
            observed_at: fallback_observed_at,
            availability: "unavailable".to_owned(),
            unavailable_reason: Some("not_captured".to_owned()),
            validation_status: None,
            protocol: None,
            cipher_suite: None,
            certificate_subject: None,
            certificate_issuer: None,
            subject_alternative_names: Vec::new(),
            certificate_not_before: None,
            certificate_not_after: None,
        });
    };
    let names = row
        .try_get::<Value, _>("tls_subject_alternative_names")
        .map_err(|_| PublicReadError::Unavailable)?;
    Ok(PublicTlsObservation {
        source,
        observed_at: row
            .try_get("tls_observed_at")
            .map_err(|_| PublicReadError::Unavailable)?,
        availability: if row
            .try_get::<Option<String>, _>("tls_unavailable_reason")
            .map_err(|_| PublicReadError::Unavailable)?
            .is_some()
        {
            "unavailable".to_owned()
        } else {
            "available".to_owned()
        },
        unavailable_reason: row
            .try_get("tls_unavailable_reason")
            .map_err(|_| PublicReadError::Unavailable)?,
        validation_status: row
            .try_get("tls_validation_status")
            .map_err(|_| PublicReadError::Unavailable)?,
        protocol: row
            .try_get("tls_protocol")
            .map_err(|_| PublicReadError::Unavailable)?,
        cipher_suite: row
            .try_get("tls_cipher_suite")
            .map_err(|_| PublicReadError::Unavailable)?,
        certificate_subject: row
            .try_get("tls_certificate_subject")
            .map_err(|_| PublicReadError::Unavailable)?,
        certificate_issuer: row
            .try_get("tls_certificate_issuer")
            .map_err(|_| PublicReadError::Unavailable)?,
        subject_alternative_names: serde_json::from_value(names)
            .map_err(|_| PublicReadError::Unavailable)?,
        certificate_not_before: row
            .try_get("tls_certificate_not_before")
            .map_err(|_| PublicReadError::Unavailable)?,
        certificate_not_after: row
            .try_get("tls_certificate_not_after")
            .map_err(|_| PublicReadError::Unavailable)?,
    })
}

fn crawl_from_row(row: sqlx::postgres::PgRow) -> Result<PublicCrawl, PublicReadError> {
    Ok(PublicCrawl {
        id: row
            .try_get::<Uuid, _>("id")
            .map_err(|_| PublicReadError::Unavailable)?
            .to_string(),
        requested_url: row
            .try_get("requested_url")
            .map_err(|_| PublicReadError::Unavailable)?,
        final_url: row
            .try_get("final_url")
            .map_err(|_| PublicReadError::Unavailable)?,
        response_status: u16::try_from(
            row.try_get::<i16, _>("response_status")
                .map_err(|_| PublicReadError::Unavailable)?,
        )
        .map_err(|_| PublicReadError::Unavailable)?,
        captured_at: row
            .try_get("captured_at")
            .map_err(|_| PublicReadError::Unavailable)?,
    })
}
fn technology_library_item_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PublicTechnologyLibraryItem, PublicReadError> {
    let net_change: i64 = row
        .try_get("net_change")
        .map_err(|_| PublicReadError::Unavailable)?;
    Ok(PublicTechnologyLibraryItem {
        slug: row
            .try_get("slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        category_slug: row
            .try_get("category_slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        category_name: row
            .try_get("category_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        adoption_count: u64::try_from(
            row.try_get::<i64, _>("adoption_count")
                .map_err(|_| PublicReadError::Unavailable)?,
        )
        .map_err(|_| PublicReadError::Unavailable)?,
        net_change,
        trend: PublicTechnologyTrend::from_net_change(net_change),
    })
}

fn count_from_row(row: &sqlx::postgres::PgRow, column: &str) -> Result<u64, PublicReadError> {
    u64::try_from(
        row.try_get::<i64, _>(column)
            .map_err(|_| PublicReadError::Unavailable)?,
    )
    .map_err(|_| PublicReadError::Unavailable)
}

fn analytics_rank_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PublicAnalyticsRank, PublicReadError> {
    Ok(PublicAnalyticsRank {
        slug: row
            .try_get("slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        count: count_from_row(&row, "count")?,
    })
}

fn country_rank_from_row(row: sqlx::postgres::PgRow) -> Result<PublicCountryRank, PublicReadError> {
    Ok(PublicCountryRank {
        country_code: row
            .try_get("country_code")
            .map_err(|_| PublicReadError::Unavailable)?,
        count: count_from_row(&row, "count")?,
    })
}

fn technology_mover_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PublicAnalyticsTechnologyMover, PublicReadError> {
    Ok(PublicAnalyticsTechnologyMover {
        slug: row
            .try_get("slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        category_slug: row
            .try_get("category_slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        category_name: row
            .try_get("category_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        adoption_count: count_from_row(&row, "adoption_count")?,
        net_change: row
            .try_get("net_change")
            .map_err(|_| PublicReadError::Unavailable)?,
    })
}

fn provider_mover_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PublicAnalyticsProviderMover, PublicReadError> {
    Ok(PublicAnalyticsProviderMover {
        slug: row
            .try_get("slug")
            .map_err(|_| PublicReadError::Unavailable)?,
        display_name: row
            .try_get("display_name")
            .map_err(|_| PublicReadError::Unavailable)?,
        adoption_count: count_from_row(&row, "adoption_count")?,
        net_change: row
            .try_get("net_change")
            .map_err(|_| PublicReadError::Unavailable)?,
    })
}

fn domain_summary_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<PublicDomainSummary, PublicReadError> {
    Ok(PublicDomainSummary {
        canonical_domain: row
            .try_get("canonical_domain")
            .map_err(|_| PublicReadError::Unavailable)?,
        last_crawled_at: row
            .try_get("last_successful_crawl_at")
            .map_err(|_| PublicReadError::Unavailable)?,
    })
}
