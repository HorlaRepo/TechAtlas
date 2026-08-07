import { test as base, expect, type Page } from "@playwright/test";

const timestamp = "2026-08-05T12:00:00Z";

const technology = {
  slug: "nextjs",
  display_name: "Next.js",
  category_slug: "framework",
  category_name: "Framework",
  adoption_count: 420,
  net_change: 21,
  trend: "growing",
};

const domain = {
  canonical_domain: "example.test",
  country_code: "US",
  last_crawled_at: timestamp,
};

function analyticsHistory(insufficient: boolean) {
  return {
    available_from: insufficient ? null : "2026-07-01",
    available_to: insufficient ? null : "2026-08-05",
    status: insufficient ? "insufficient_history" : "ready",
    series: insufficient
      ? []
      : [
          { slug: "nextjs", display_name: "Next.js", points: [{ day: "2026-07-01", count: 390 }, { day: "2026-08-05", count: 420 }] },
          { slug: "react", display_name: "React", points: [{ day: "2026-07-01", count: 360 }, { day: "2026-08-05", count: 371 }] },
        ],
  };
}

function responseFor(path: string, search: URLSearchParams, method: string): unknown {
  const insufficientHistory = search.get("since_days") === "365";

  if (path === "/api/v1/public/analytics/overview") return { domain_count: 1250, technology_count: 96, current_detection_count: 1730, country_count: 24 };
  if (path === "/api/v1/public/analytics/rankings") return { technologies: [{ slug: "nextjs", display_name: "Next.js", count: 420 }], providers: [{ slug: "vercel", display_name: "Vercel", count: 215 }], categories: [{ slug: "framework", display_name: "Framework", count: 420 }], countries: [{ country_code: "US", count: 610 }] };
  if (path === "/api/v1/public/analytics/movers") return { growing_technologies: [technology], declining_technologies: [], growing_providers: [{ slug: "vercel", display_name: "Vercel", adoption_count: 215, net_change: 12 }], declining_providers: [] };
  if (path === "/api/v1/public/analytics/changes") return insufficientHistory ? [] : [{ day: "2026-08-04", added: 4, removed: 1, migrated: 1 }, { day: "2026-08-05", added: 7, removed: 2, migrated: 3 }];
  if (path === "/api/v1/public/analytics/adoption-history") return analyticsHistory(insufficientHistory);
  if (path === "/api/v1/public/analytics/adoption") return [{ slug: "nextjs", count: 420 }];
  if (path === "/api/v1/public/analytics/discovery") return { large_migrations: [{ from_slug: "gatsby", from_display_name: "Gatsby", to_slug: "nextjs", to_display_name: "Next.js", domain_count: 18 }], newest_domains: [{ canonical_domain: "new.example.test", first_indexed_at: timestamp }], frequently_crawled_domains: [{ canonical_domain: "example.test", crawl_count: 8, last_crawled_at: timestamp }] };

  if (path === "/api/v1/public/search/domains") return { results: [{ ...domain, technology_slugs: ["nextjs"], category_slugs: ["framework"], max_confidence: 92, updated_at: timestamp }], facets: { technology: { nextjs: 1 }, category: { framework: 1 }, country: { US: 1 } }, estimated_total_hits: 1, limit: 20, offset: 0, query_at: timestamp };
  if (path === "/api/v1/public/compare") return { domains: ["alpha.test", "beta.test"], cells: [{ canonical_domain: "alpha.test", category_slug: "framework", technology_slug: "nextjs", state: "current", last_observed_at: timestamp }, { canonical_domain: "beta.test", category_slug: "framework", technology_slug: "nextjs", state: "current", last_observed_at: timestamp }] };
  if (path === "/api/v1/public/technologies") return { categories: [{ slug: "framework", display_name: "Framework" }], items: [technology], next_cursor: null };
  if (path.endsWith("/profile") && path.startsWith("/api/v1/public/technologies/")) return { ...technology, history: [{ day: "2026-08-01", net_change: 4 }, { day: "2026-08-05", net_change: 7 }], related_technologies: [{ slug: "react", display_name: "React", category_slug: "library", category_name: "Library", shared_domain_count: 210 }], domains: { items: [domain], next_cursor: null } };
  if (path.startsWith("/api/v1/public/technologies/")) return { items: [domain], next_cursor: null };
  if (path.startsWith("/api/v1/public/providers/")) return { slug: "vercel", display_name: "Vercel", adoption_count: 215, net_change: 12, technologies: [technology], trend: "growing", domains: { items: [domain], next_cursor: null } };
  if (path.endsWith("/changes") && path.startsWith("/api/v1/public/domains/")) return { items: [{ category_slug: "framework", kind: "added", technology_slug: "nextjs", observed_at: timestamp }], next_cursor: null };
  if (path.endsWith("/crawls") && path.startsWith("/api/v1/public/domains/")) return { items: [{ id: "crawl-1", requested_url: "https://example.test", final_url: "https://example.test/", response_status: 200, captured_at: timestamp }], next_cursor: null };
  if (path.includes("/crawls/") && path.startsWith("/api/v1/public/domains/")) return { crawl: { id: "crawl-1", requested_url: "https://example.test", final_url: "https://example.test/", response_status: 200, captured_at: timestamp }, dns: { availability: "available", addresses: ["203.0.113.10"], observed_at: timestamp, source: "fixture" }, tls: { availability: "available", observed_at: timestamp, source: "fixture", subject_alternative_names: [] }, redirect_chain: [], response_headers: {} };
  if (path.startsWith("/api/v1/public/domains/")) return { canonical_domain: "example.test", country_code: "US", first_indexed_at: timestamp, last_crawled_at: timestamp, technologies: [{ slug: "nextjs", display_name: "Next.js", category_slug: "framework", category_name: "Framework", confidence: 92, method: "deterministic_rule", rule_version: 1, first_observed_at: timestamp, last_observed_at: timestamp, evidence: [{ source: "header", key: "x-powered-by", value: "Next.js" }] }] };

  if (path === "/api/v1/admin/operations/overview") return { domain_count: 1250, successful_crawl_count: 980, technology_count: 96, current_detection_count: 1730, generated_at: timestamp, system_status: "online", dependencies: [{ name: "PostgreSQL", status: "healthy" }, { name: "Redis", status: "healthy" }], queue: { ready_count: 3, processing_count: 1, scheduled_count: 4 }, workers: [{ name: "worker-1", status: "healthy", region: "local", in_flight_work: 1, completed_total: 42, last_heartbeat_at: timestamp }], activity: [], alertable_failures: [], throughput: [] };
  if (path === "/api/v1/admin/domains") return method === "GET" ? { items: [{ canonical_domain: "example.test", created_at: timestamp, policy: { desired_interval_hours: 168, is_enabled: true, priority: "medium" } }], next_cursor: null } : { canonical_domain: "example.test", created_at: timestamp };
  if (path === "/api/v1/admin/crawl-attempts") return { items: [], next_cursor: null };
  if (path === "/api/v1/admin/audit-events") return { items: [], next_cursor: null };
  if (path === "/api/v1/admin/detection-rules") return { rules: [] };
  if (path === "/api/v1/admin/detection-reprocessing-runs") return { runs: [] };

  return {};
}

export async function installApiFixtures(page: Page) {
  await page.route("**/api/v1/**", async (route) => {
    const requestUrl = new URL(route.request().url());
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(responseFor(requestUrl.pathname, requestUrl.searchParams, route.request().method())),
    });
  });
}

export const test = base.extend({
  page: async ({ page }, use) => {
    await installApiFixtures(page);
    await use(page);
  },
});

export { expect };
