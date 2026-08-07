import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { AdminImportsPage } from "./admin-imports-page";

vi.mock("@techatlas/api-client", () => ({
  importCsv: vi.fn(),
  completedImportBatches: vi.fn(async () => ({ data: { imports: [] } })),
  scheduleImportBatchRecrawl: vi.fn(),
}));

describe("AdminImportsPage", () => {
  it("loads a selected CSV file into the import payload", async () => {
    renderPage();
    const file = new File(["domain\nexample.com\n"], "domains.csv", { type: "text/csv" });
    Object.defineProperty(file, "text", { value: async () => "domain\nexample.com\n" });

    fireEvent.change(screen.getByLabelText("Choose CSV file"), { target: { files: [file] } });

    expect(await screen.findByText("Loaded domains.csv")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "CSV content" })).toHaveValue("domain\nexample.com\n");
  });

  it("rejects a non-CSV upload before it reaches the importer", async () => {
    renderPage();
    const file = new File(["not a CSV"], "domains.txt", { type: "text/plain" });

    fireEvent.change(screen.getByLabelText("Choose CSV file"), { target: { files: [file] } });

    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a CSV file.");
  });

  it("schedules a selected completed batch after confirmation", async () => {
    const api = await import("@techatlas/api-client");
    vi.mocked(api.completedImportBatches).mockResolvedValue({
      data: {
        imports: [{
          import_id: "batch-123",
          source_name: "Top 500",
          completed_at: "2026-08-07T12:00:00Z",
          domain_count: 500,
        }],
      },
    } as never);
    vi.mocked(api.scheduleImportBatchRecrawl).mockResolvedValue({
      data: {
        import_id: "batch-123",
        source_name: "Top 500",
        requested_domain_count: 500,
        scheduled_domain_count: 490,
        skipped_domain_count: 10,
      },
    } as never);
    renderPage();

    fireEvent.click(await screen.findByRole("button", { name: "Recrawl batch" }));
    fireEvent.click(screen.getByRole("button", { name: "Schedule batch recrawl" }));

    expect(await screen.findByRole("status")).toHaveTextContent("Scheduled 490 of 500 domains from Top 500; 10 skipped.");
    expect(api.scheduleImportBatchRecrawl).toHaveBeenCalledWith({ path: { import_id: "batch-123" } });
  });
});

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <AdminImportsPage />
    </QueryClientProvider>,
  );
}
