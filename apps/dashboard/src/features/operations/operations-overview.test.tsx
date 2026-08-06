import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "@/app/app";

const { logout } = vi.hoisted(() => ({ logout: vi.fn() }));

vi.mock("@auth0/auth0-react", () => ({
  useAuth0: () => ({
    getAccessTokenSilently: vi.fn(async () => "test-access-token"),
    isAuthenticated: true,
    isLoading: false,
    logout,
    user: { email: "operator@example.test", name: "Operations Admin" },
  }),
}));

vi.mock("@/app/auth0", () => ({
  auth0Configuration: {
    audience: "https://api.techatlas",
    clientId: "test-client-id",
    domain: "example.us.auth0.com",
    scope: "openid profile email admin:read admin:operate",
  },
}));

const overview = {
  generated_at: "2026-08-04T10:00:00Z",
  system_status: "online",
  domain_count: 2400000,
  technology_count: 452,
  current_detection_count: 8100000,
  successful_crawl_count: 12800000,
  queue: { ready_count: 39208, processing_count: 27414, scheduled_count: 18619 },
  throughput: [{ observed_at: "2026-08-04T10:00:00Z", completed_count: 1200 }],
  workers: [{ name: "atlas-worker-01", region: "eu-west", in_flight_work: 2, completed_total: 82031, status: "healthy", last_heartbeat_at: "2026-08-04T10:00:00Z" }],
  activity: [],
};

vi.mock("@techatlas/api-client", () => ({
  client: { setConfig: vi.fn() },
  operationsOverview: vi.fn(async () => ({ data: overview, error: undefined, response: new Response() })),
  requestDomainCrawl: vi.fn(async () => ({ data: { canonical_domain: "example.com", scheduled_at: "2026-08-05T00:00:00Z" }, error: undefined, response: new Response() })),
}));

describe("operations dashboard", () => {
  beforeEach(() => {
    logout.mockReset();
    window.history.replaceState({}, "", "/admin/overview");
  });

  it("renders every core telemetry metric", async () => {
    render(<App />);

    await screen.findByRole("heading", { name: "Operations Center" });

    expect(screen.getByText("TOTAL DOMAINS")).toBeInTheDocument();
    expect(screen.getAllByText("2.4M").length).toBeGreaterThan(0);
    expect(screen.getByText("SUCCESSFUL CRAWLS")).toBeInTheDocument();
    expect(screen.getByText("TECH DETECTED")).toBeInTheDocument();
  });

  it("focuses global search from the keyboard shortcut and requests a quick crawl", async () => {
    render(<App />);

    await screen.findByRole("heading", { name: "Operations Center" });

    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("textbox", { name: "Search TechAtlas" })).toHaveFocus();

    fireEvent.click(screen.getByRole("button", { name: "Quick Crawl" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Existing enabled domain" }), { target: { value: "example.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Request crawl" }));
    expect(await screen.findByText("example.com is eligible for scheduler processing.")).toBeInTheDocument();
  });

  it("keeps sign out in the account settings popover", async () => {
    render(<App />);

    await screen.findByRole("heading", { name: "Operations Center" });

    expect(screen.getByText("Operations Admin")).toBeInTheDocument();
    expect(screen.getByText("operator@example.test")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign out" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Account settings" }));
    expect(screen.getByRole("dialog", { name: "Account settings" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(logout).toHaveBeenCalledWith({ logoutParams: { returnTo: window.location.origin } });
  });
});
