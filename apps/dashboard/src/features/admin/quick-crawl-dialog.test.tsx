import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { QuickCrawlDialog } from "./quick-crawl-dialog";

const api = vi.hoisted(() => ({ requestDomainCrawl: vi.fn() }));

vi.mock("@techatlas/api-client", () => api);

describe("QuickCrawlDialog", () => {
  it("submits the generated protected command and reports scheduler eligibility", async () => {
    api.requestDomainCrawl.mockResolvedValue({
      data: { canonical_domain: "example.com", scheduled_at: "2026-08-05T00:00:00Z" },
    });
    renderDialog();

    fireEvent.change(screen.getByRole("textbox", { name: "Existing enabled domain" }), {
      target: { value: "example.com" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Request crawl" }));

    expect(await screen.findByText("example.com is eligible for scheduler processing.")).toBeInTheDocument();
    expect(api.requestDomainCrawl).toHaveBeenCalledWith({ path: { canonical_domain: "example.com" } });
  });

  it("explains why a disabled domain cannot be requested", async () => {
    api.requestDomainCrawl.mockResolvedValue({ error: {}, response: { status: 409 } });
    renderDialog();

    fireEvent.change(screen.getByRole("textbox", { name: "Existing enabled domain" }), {
      target: { value: "disabled.example" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Request crawl" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Crawls are disabled for this domain.");
  });
});

function renderDialog() {
  const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <QuickCrawlDialog onClose={() => undefined} />
    </QueryClientProvider>,
  );
}
