import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { AdminRuntimePage } from "./admin-runtime-page";

const api = vi.hoisted(() => ({
  crawlAttempts: vi.fn(),
  retryCrawlAttempt: vi.fn(),
  scheduleCountryEnrichmentRecrawl: vi.fn(),
}));

vi.mock("@techatlas/api-client", () => api);
vi.mock("@/features/operations/operations-query", () => ({
  useOperationsOverview: () => ({
    isLoading: false,
    isError: false,
    data: { workers: [], queue: { ready_count: 0, processing_count: 0, scheduled_count: 0 } },
  }),
}));

describe("AdminRuntimePage", () => {
  it("explains automatic and policy-blocked retry states", async () => {
    api.crawlAttempts.mockResolvedValue({
      data: {
        attempts: [
          attempt({ job_id: "automatic", retry_state: "automatic_retry_scheduled", retry_eligible: false }),
          attempt({ job_id: "blocked", retry_state: "manual_retry_blocked", retry_eligible: false, failure_code: "robots_denied" }),
        ],
      },
    });
    renderPage();

    expect(await screen.findByText("Automatic retry scheduled")).toBeInTheDocument();
    expect(screen.getByText("Manual retry blocked by crawl policy")).toBeInTheDocument();
    expect(screen.queryByText("Retry unavailable")).not.toBeInTheDocument();
  });

  it("confirms and reports the country enrichment recrawl count", async () => {
    api.crawlAttempts.mockResolvedValue({ data: { attempts: [] } });
    api.scheduleCountryEnrichmentRecrawl.mockResolvedValue({ data: { scheduled_domain_count: 2 } });
    renderPage();

    fireEvent.click(screen.getByRole("button", { name: "Recrawl domains missing country data" }));
    expect(await screen.findByRole("dialog", { name: "Recrawl domains missing country data" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Schedule country recrawls" }));

    expect(await screen.findByText("Scheduled 2 domains for country enrichment.")).toBeInTheDocument();
  });
});

function attempt(overrides: Partial<Record<string, unknown>>) {
  return {
    attempt_number: 3,
    canonical_domain: "example.com",
    failure_code: "network_timeout",
    failure_summary: "Timed out while connecting.",
    finished_at: "2026-08-04T20:00:00Z",
    job_id: "attempt",
    queued_at: "2026-08-04T19:00:00Z",
    retry_eligible: false,
    retry_state: "not_applicable",
    status: "failed",
    ...overrides,
  };
}

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={queryClient}><AdminRuntimePage kind="scheduler" /></QueryClientProvider>);
}
