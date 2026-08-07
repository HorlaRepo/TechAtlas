import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { AnchorHTMLAttributes, ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { DomainComparisonPage } from "./domain-comparison-page";
import { addComparisonDomain, validateComparisonState } from "./comparison-state";

const { compare, searchDomains } = vi.hoisted(() => ({
  compare: vi.fn(async () => ({
    data: {
      domains: ["alpha.test", "beta.test"],
      cells: [
        { category_slug: "frontend-framework", canonical_domain: "alpha.test", technology_slug: "react", state: "current", last_observed_at: "2026-08-04T10:00:00Z" },
        { category_slug: "frontend-framework", canonical_domain: "beta.test", technology_slug: "react", state: "current", last_observed_at: "2026-08-04T10:00:00Z" },
        { category_slug: "cms", canonical_domain: "alpha.test", technology_slug: "wordpress", state: "current", last_observed_at: "2026-08-04T10:00:00Z" },
        { category_slug: "cms", canonical_domain: "beta.test", technology_slug: null, state: "unknown", last_observed_at: null },
      ],
    },
    error: undefined,
    response: new Response(),
  })),
  searchDomains: vi.fn(async () => ({
    data: {
      results: [{ canonical_domain: "gamma.test", technology_slugs: ["nextjs"], category_slugs: ["frontend-framework"], country_code: null, max_confidence: 95, last_crawled_at: null, updated_at: "2026-08-04T10:00:00Z" }],
      estimated_total_hits: 1,
      facets: { technology: {}, category: {}, country: {} },
      limit: 5,
      offset: 0,
      query_at: "2026-08-04T10:00:00Z",
    },
    error: undefined,
    response: new Response(),
  })),
}));

vi.mock("@techatlas/api-client", () => ({ compare, searchDomains }));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ to, params, children, ...props }: { to: string; params?: Record<string, string>; children?: ReactNode } & AnchorHTMLAttributes<HTMLAnchorElement>) => <a href={to.replace(/\$([a-z_]+)/g, (_, key: string) => params?.[key] ?? "")} {...props}>{children}</a>,
}));

function renderComparison(onSearchChange = vi.fn()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={queryClient}><DomainComparisonPage search={{ domain: ["alpha.test", "beta.test"] }} onSearchChange={onSearchChange} /></QueryClientProvider>);
  return onSearchChange;
}

describe("domain comparison", () => {
  it("normalizes repeatable shareable domain state", () => {
    expect(validateComparisonState({ domain: ["https://Alpha.test/path", "beta.test", "ALPHA.test"] })).toEqual({ domain: ["alpha.test", "beta.test"] });
    expect(addComparisonDomain({ domain: ["alpha.test"] }, "beta.test")).toEqual({ domain: ["alpha.test", "beta.test"] });
  });

  it("renders normalized category states and offers manual and search-backed selection", async () => {
    const onSearchChange = renderComparison();

    await screen.findByRole("heading", { name: "Technology comparison" });
    expect(compare).toHaveBeenCalledWith({ query: { domain: ["alpha.test", "beta.test"] } });
    expect(screen.getAllByText("Common").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Unique").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Unknown").length).toBeGreaterThan(0);
    expect(screen.getAllByRole("link", { name: /alpha\.test/ })[0]).toHaveAttribute("href", "/domains/alpha.test");

    fireEvent.change(screen.getByLabelText("Add a domain"), { target: { value: "gamma.test" } });
    fireEvent.click(screen.getByRole("button", { name: "Add domain" }));
    expect(onSearchChange).toHaveBeenLastCalledWith({ domain: ["alpha.test", "beta.test", "gamma.test"] });

    fireEvent.change(screen.getByLabelText("Find an indexed domain"), { target: { value: "gamma" } });
    await screen.findByRole("button", { name: /gamma\.test/ });
    expect(searchDomains).toHaveBeenCalledWith({ query: { q: "gamma", limit: 5 } });
  });
});
