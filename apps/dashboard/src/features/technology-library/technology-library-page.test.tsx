import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { AnchorHTMLAttributes, ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { TechnologyLibraryPage } from "./technology-library-page";
import { currentTechnologyLibraryView, validateTechnologyLibraryState } from "./technology-library-state";

const { technologies } = vi.hoisted(() => ({
  technologies: vi.fn(async () => ({
    data: {
      items: [{
        slug: "react",
        display_name: "React",
        category_slug: "frontend-framework",
        category_name: "Frontend framework",
        adoption_count: 37,
        net_change: 4,
        trend: "growing",
      }],
      categories: [{ slug: "frontend-framework", display_name: "Frontend framework" }],
      next_cursor: undefined,
    },
    error: undefined,
    response: new Response(),
  })),
}));

vi.mock("@techatlas/api-client", () => ({ technologies }));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ to, params, children, ...props }: { to: string; params?: Record<string, string>; children?: ReactNode } & AnchorHTMLAttributes<HTMLAnchorElement>) => <a href={to.replace(/\$([a-z_]+)/g, (_, key: string) => params?.[key] ?? "")} {...props}>{children}</a>,
}));

function renderTechnologyLibrary(onSearchChange = vi.fn()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <TechnologyLibraryPage search={{ category: "frontend-framework", trend: "growing" }} onSearchChange={onSearchChange} />
    </QueryClientProvider>,
  );
  return onSearchChange;
}

describe("technology library", () => {
  it("validates URL-addressable category, trend, and display state", () => {
    expect(validateTechnologyLibraryState({ category: "frontend-framework", trend: "growing", view: "list" })).toEqual({ category: "frontend-framework", trend: "growing", view: "list" });
    expect(validateTechnologyLibraryState({ category: "React", trend: "sideways", view: "grid" })).toEqual({});
    expect(currentTechnologyLibraryView({})).toBe("grid");
  });

  it("sends filters to the generated client and switches to a URL-backed list view", async () => {
    const onSearchChange = renderTechnologyLibrary();

    await screen.findByRole("heading", { name: "1 technologies" });
    expect(technologies).toHaveBeenCalledWith({ query: { category: "frontend-framework", trend: "growing" } });
    expect(screen.getByText("React")).toBeInTheDocument();
    expect(screen.getByText("+4")).toBeInTheDocument();
    expect(screen.getAllByText("Growing")).toHaveLength(2);

    fireEvent.click(screen.getByRole("button", { name: "List" }));
    expect(onSearchChange).toHaveBeenLastCalledWith({ category: "frontend-framework", trend: "growing", view: "list" });

    fireEvent.change(screen.getByLabelText("Trend"), { target: { value: "stable" } });
    expect(onSearchChange).toHaveBeenLastCalledWith({ category: "frontend-framework", trend: "stable" });
  });
});
