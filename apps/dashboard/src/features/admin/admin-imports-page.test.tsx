import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { AdminImportsPage } from "./admin-imports-page";

vi.mock("@techatlas/api-client", () => ({
  importCsv: vi.fn(),
}));

describe("AdminImportsPage", () => {
  it("loads a selected CSV file into the import payload", async () => {
    renderPage();
    const file = new File(["domain\nexample.com\n"], "domains.csv", { type: "text/csv" });
    Object.defineProperty(file, "text", { value: async () => "domain\nexample.com\n" });

    fireEvent.change(screen.getByLabelText("Choose CSV file"), { target: { files: [file] } });

    expect(await screen.findByRole("status")).toHaveTextContent("Loaded domains.csv");
    expect(screen.getByRole("textbox", { name: "CSV content" })).toHaveValue("domain\nexample.com\n");
  });

  it("rejects a non-CSV upload before it reaches the importer", async () => {
    renderPage();
    const file = new File(["not a CSV"], "domains.txt", { type: "text/plain" });

    fireEvent.change(screen.getByLabelText("Choose CSV file"), { target: { files: [file] } });

    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a CSV file.");
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
