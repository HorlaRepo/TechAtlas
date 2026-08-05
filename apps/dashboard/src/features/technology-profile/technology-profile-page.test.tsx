import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { AnchorHTMLAttributes, ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { TechnologyProfilePage } from "./technology-profile-page";

const { technologyProfile } = vi.hoisted(() => ({
  technologyProfile: vi.fn(async () => ({
    data: {
      slug: "react",
      display_name: "React",
      category_slug: "frontend-framework",
      category_name: "Frontend framework",
      adoption_count: 37,
      net_change: 4,
      trend: "growing",
      history: [{ day: "2026-08-04", net_change: 4 }],
      related_technologies: [{
        slug: "nextjs",
        display_name: "Next.js",
        category_slug: "frontend-framework",
        category_name: "Frontend framework",
        shared_domain_count: 12,
      }],
      domains: {
        items: [{ canonical_domain: "example.test", last_crawled_at: "2026-08-04T10:00:00Z" }],
        next_cursor: undefined,
      },
    },
    error: undefined,
    response: new Response(),
  })),
}));

vi.mock("@techatlas/api-client", () => ({ technologyProfile }));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ to, params, children, ...props }: { to: string; params?: Record<string, string>; children?: ReactNode } & AnchorHTMLAttributes<HTMLAnchorElement>) => <a href={to.replace(/\$([a-z_]+)/g, (_, key: string) => params?.[key] ?? "")} {...props}>{children}</a>,
}));

describe("technology profile", () => {
  it("renders adoption, history, related technology, and current domains from the typed profile", async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(<QueryClientProvider client={queryClient}><TechnologyProfilePage technology="react" /></QueryClientProvider>);

    await screen.findByRole("heading", { name: "React" });
    expect(technologyProfile).toHaveBeenCalledWith({ path: { technology_slug: "react" }, query: {} });
    expect(screen.getByText("37 domains")).toBeInTheDocument();
    expect(screen.getAllByText("+4")).toHaveLength(2);
    expect(screen.getByText("Aug 4")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /Next\.js/ })).toHaveAttribute("href", "/technologies/nextjs");
    expect(screen.getByRole("link", { name: /example\.test/ })).toHaveAttribute("href", "/domains/example.test");
  });
});
