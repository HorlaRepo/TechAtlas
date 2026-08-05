import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminMonitoringPage } from "./admin-monitoring-page";

const useOverview = vi.hoisted(() => vi.fn());

vi.mock("@/app/observability", () => ({
  configuredObservabilityUrl: () => "https://grafana.example.test/ops",
}));

vi.mock("@/features/operations/operations-query", () => ({
  OperationsOverviewError: class OperationsOverviewError extends Error {
    get isUnavailable() {
      return false;
    }
  },
  useOperationsOverview: () => useOverview(),
}));

describe("AdminMonitoringPage", () => {
  beforeEach(() => {
    useOverview.mockReturnValue({
      isLoading: false,
      isError: false,
      data: {
        dependencies: [
          { name: "postgres", status: "ready" },
          { name: "redis", status: "unavailable" },
        ],
        alertable_failures: [
          { code: "dependency_unavailable", severity: "critical", message: "redis is unavailable." },
        ],
        queue: { ready_count: 4, processing_count: 2, scheduled_count: 1 },
        workers: [
          { name: "atlas-worker-01", status: "healthy" },
          { name: "atlas-worker-02", status: "stale" },
        ],
      },
    });
  });

  it("renders dependency readiness, alert conditions, and the configured observability link", () => {
    render(<AdminMonitoringPage />);

    expect(screen.getByRole("heading", { name: "Monitoring" })).toBeInTheDocument();
    expect(screen.getByText("postgres")).toBeInTheDocument();
    expect(screen.getByText("redis is unavailable.")).toBeInTheDocument();
    expect(screen.getByText("1 healthy worker · 1 stale.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open observability" })).toHaveAttribute("href", "https://grafana.example.test/ops");
  });
});
