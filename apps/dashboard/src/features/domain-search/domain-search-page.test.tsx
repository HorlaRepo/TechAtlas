import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { DomainSearchPage } from "./domain-search-page";
import { parseSearchParams, stringifyRouterSearch, stringifySearchParams, validateDomainSearchState } from "./search-state";

const { searchDomains } = vi.hoisted(() => ({
  searchDomains: vi.fn(async () => ({
    data: {
      results: [{
        canonical_domain: "example.test",
        technology_slugs: ["react", "nextjs"],
        category_slugs: ["frontend-framework"],
        country_code: "NG",
        last_crawled_at: "2026-08-04T10:00:00Z",
        updated_at: "2026-08-04T11:00:00Z",
      }],
      estimated_total_hits: 51,
      facets: {
        technology: { react: 37, nextjs: 22 },
        category: { "frontend-framework": 42 },
        country: { NG: 13, US: 9 },
      },
      limit: 50,
      offset: 0,
      query_at: "2026-08-04T12:00:00Z",
    },
    error: undefined,
    response: new Response(),
  })),
}));

vi.mock("@techatlas/api-client", () => ({ searchDomains }));

function renderSearch(onSearchChange = vi.fn()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <DomainSearchPage search={{ q: "example", technology: ["react"], sort: "updated_desc" }} onSearchChange={onSearchChange} />
    </QueryClientProvider>,
  );
  return onSearchChange;
}

describe("domain search", () => {
  it("serializes repeated filter values for shareable API URLs", () => {
    const search = validateDomainSearchState(parseSearchParams("?q=example&technology=react&technology=nextjs&country=NG&sort=updated_desc"));

    expect(search).toEqual({ q: "example", technology: ["react", "nextjs"], country: ["NG"], sort: "updated_desc" });
    expect(stringifySearchParams(search)).toBe("q=example&technology=react&technology=nextjs&country=NG&sort=updated_desc");
    expect(stringifyRouterSearch(search)).toBe("?q=example&technology=react&technology=nextjs&country=NG&sort=updated_desc");
  });

  it("sends the URL-backed state to the generated client and paginates results", async () => {
    const onSearchChange = renderSearch();

    await screen.findByRole("heading", { name: "51 estimated domains" });
    expect(searchDomains).toHaveBeenCalledWith({ query: { q: "example", technology: ["react"], sort: "updated_desc" } });
    expect(screen.getAllByText("example.test").length).toBeGreaterThan(0);
    expect(screen.getAllByRole("link", { name: /example.test/ })[0]).toHaveAttribute("href", "/domains/example.test");

    fireEvent.click(screen.getByRole("button", { name: /next/i }));
    expect(onSearchChange).toHaveBeenCalledWith({ q: "example", technology: ["react"], sort: "updated_desc", offset: 50 });
  });

  it("applies facet changes immediately and removes filters individually", async () => {
    const onSearchChange = renderSearch();

    await screen.findByRole("heading", { name: "51 estimated domains" });
    fireEvent.click(screen.getByRole("button", { name: /country/i }));
    fireEvent.click(screen.getByRole("checkbox", { name: /NG/i }));

    expect(onSearchChange).toHaveBeenLastCalledWith({ q: "example", technology: ["react"], country: ["NG"], sort: "updated_desc" });

    fireEvent.click(screen.getByRole("button", { name: "Remove Technology: react filter" }));
    expect(onSearchChange).toHaveBeenLastCalledWith({ q: "example", sort: "updated_desc" });
  });

  it("keeps facet controls keyboard accessible and resets pagination for search and preset filters", async () => {
    const onSearchChange = renderSearch();

    await screen.findByRole("heading", { name: "51 estimated domains" });
    const technologyButton = screen.getByRole("button", { name: /^Technology/ });
    fireEvent.click(technologyButton);
    const technologyMenu = screen.getByRole("group", { name: "Technology filters" });
    fireEvent.keyDown(technologyMenu, { key: "Escape" });
    expect(screen.queryByRole("group", { name: "Technology filters" })).not.toBeInTheDocument();
    expect(document.activeElement).toBe(technologyButton);

    fireEvent.change(screen.getByLabelText("Sort results"), { target: { value: "last_crawled_desc" } });
    expect(onSearchChange).toHaveBeenLastCalledWith({ q: "example", technology: ["react"], sort: "last_crawled_desc" });

    fireEvent.change(screen.getByLabelText("Minimum confidence"), { target: { value: "70" } });
    expect(onSearchChange).toHaveBeenLastCalledWith({ q: "example", technology: ["react"], min_confidence: 70, sort: "updated_desc" });

    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect(onSearchChange).toHaveBeenLastCalledWith({ q: "example", technology: ["react"], sort: "updated_desc" });
  });
});
