import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DomainProfilePage } from "./domain-profile-page";

const { crawlDetail, domainChanges, domainCrawls, domainProfile, requestRefresh } = vi.hoisted(() => ({
  crawlDetail: vi.fn(),
  domainChanges: vi.fn(),
  domainCrawls: vi.fn(),
  domainProfile: vi.fn(),
  requestRefresh: vi.fn(),
}));

vi.mock("@techatlas/api-client", () => ({ crawlDetail, domainChanges, domainCrawls, domainProfile, requestRefresh }));

const profile = {
  canonical_domain: "example.test",
  first_indexed_at: "2026-08-01T08:00:00Z",
  last_crawled_at: "2026-08-04T10:00:00Z",
  country_code: "NG",
  technologies: [{
    slug: "react",
    display_name: "React",
    category_slug: "frontend-framework",
    category_name: "Frontend framework",
    confidence: 95,
    method: "header",
    rule_version: 3,
    first_observed_at: "2026-08-01T08:00:00Z",
    last_observed_at: "2026-08-04T10:00:00Z",
    evidence: [{ source: "header", key: "x-powered-by", value: "React" }],
  }],
};

function renderProfile(domain = "example.test") {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={queryClient}><DomainProfilePage domain={domain} /></QueryClientProvider>);
}

beforeEach(() => {
  domainChanges.mockResolvedValue({ data: { items: [], next_cursor: null }, error: undefined, response: new Response() });
  domainCrawls.mockResolvedValue({ data: { items: [], next_cursor: null }, error: undefined, response: new Response() });
  requestRefresh.mockResolvedValue({ data: { accepted_at: "2026-08-04T10:00:00Z", next_allowed_at: "2026-08-05T10:00:00Z" }, error: undefined, response: new Response() });
});

describe("domain profile", () => {
  it("renders current observations and reveals redacted evidence on demand", async () => {
    domainProfile.mockResolvedValue({ data: profile, error: undefined, response: new Response() });

    renderProfile();

    await screen.findByRole("heading", { name: "example.test" });
    expect(domainProfile).toHaveBeenCalledWith({ path: { canonical_domain: "example.test" } });
    expect(screen.getByText("Frontend framework")).toBeInTheDocument();
    expect(screen.getByLabelText("Confidence 95%")).toBeInTheDocument();

    const summary = screen.getAllByText("React")[0].closest("summary");
    expect(summary).not.toBeNull();
    expect(summary?.parentElement).not.toHaveAttribute("open");
    fireEvent.click(summary!);
    expect(summary?.parentElement).toHaveAttribute("open");
    expect(screen.getByText("header · x-powered-by")).toBeInTheDocument();
    expect(screen.getByText("Evidence (redacted)")).toBeInTheDocument();
  });

  it("distinguishes unknown and absent profile data", async () => {
    domainProfile.mockResolvedValue({ data: { ...profile, canonical_domain: "unknown-data.test", country_code: null, last_crawled_at: null, technologies: [] }, error: undefined, response: new Response() });

    renderProfile("unknown-data.test");

    await screen.findByRole("heading", { name: "unknown-data.test" });
    expect(screen.getByText("Country unavailable")).toBeInTheDocument();
    expect(screen.getAllByText("Awaiting first crawl").length).toBeGreaterThan(0);
    expect(screen.getByRole("heading", { name: "No current technology detections" })).toBeInTheDocument();
    expect(screen.getByText(/not proof that the domain does not use any technology/i)).toBeInTheDocument();
  });

  it("keeps a missing profile inside a dedicated not-found state", async () => {
    domainProfile.mockResolvedValue({ data: undefined, error: { code: "not_found" }, response: new Response(null, { status: 404 }) });

    renderProfile("missing.test");

    expect(await screen.findByRole("heading", { name: "Domain not found" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to search" })).toHaveAttribute("href", "/search");
  });

  it("renders typed history, fetches crawl metadata on demand, and reports refresh acceptance", async () => {
    domainProfile.mockResolvedValue({ data: profile, error: undefined, response: new Response() });
    domainChanges.mockResolvedValue({ data: { items: [{ kind: "added", category_slug: "frontend-framework", from_technology_slug: null, to_technology_slug: "react", observed_at: "2026-08-04T10:00:00Z" }], next_cursor: null }, error: undefined, response: new Response() });
    domainCrawls.mockResolvedValue({ data: { items: [{ id: "crawl-1", requested_url: "https://example.test", final_url: "https://www.example.test", response_status: 200, captured_at: "2026-08-04T10:00:00Z" }], next_cursor: null }, error: undefined, response: new Response() });
    crawlDetail.mockResolvedValue({ data: { crawl: { id: "crawl-1", requested_url: "https://example.test", final_url: "https://www.example.test", response_status: 200, captured_at: "2026-08-04T10:00:00Z" }, country_code: "NG", redirect_chain: [{ from_url: "https://example.test", to_url: "https://www.example.test", status: 301 }], response_headers: { "content-type": "text/html" }, dns: { source: "crawler_dns_v1", observed_at: "2026-08-04T10:00:00Z", availability: "available", unavailable_reason: null, queried_name: "www.example.test", addresses: ["93.184.216.34"] }, tls: { source: "crawler_tls_v1", observed_at: "2026-08-04T10:00:00Z", availability: "available", unavailable_reason: null, validation_status: "validation_failed", protocol: "TLS 1.3", cipher_suite: "TLS13_AES_256_GCM_SHA384", certificate_subject: "www.example.test", certificate_issuer: "Example Certificate Authority", subject_alternative_names: ["www.example.test"], certificate_not_before: "2026-01-01T00:00:00Z", certificate_not_after: "2027-01-01T00:00:00Z" } }, error: undefined, response: new Response() });

    renderProfile();

    expect(await screen.findByRole("heading", { name: "Technology history" })).toBeInTheDocument();
    expect(await screen.findByText("react was added")).toBeInTheDocument();
    const crawlSummary = screen.getByText("https://www.example.test").closest("summary");
    expect(crawlSummary).not.toBeNull();
    fireEvent.click(crawlSummary!);
    expect(await screen.findByText("Response headers (sanitized)")).toBeInTheDocument();
    expect(screen.getByText("content-type")).toBeInTheDocument();
    expect(screen.getByText("Validation failed — certificate facts were observed without trusting the chain.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Request a refresh" }));
    expect(await screen.findByText(/Refresh recorded for scheduler processing/i)).toBeInTheDocument();
    expect(requestRefresh).toHaveBeenCalledWith({ path: { canonical_domain: "example.test" } });
  });
});
