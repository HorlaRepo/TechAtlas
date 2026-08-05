import { operationsOverview, type OperationsOverviewResponse } from "@techatlas/api-client";
import { useQuery } from "@tanstack/react-query";

export class OperationsOverviewError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Operations data is unavailable.");
    this.status = status;
  }

  get isUnavailable() {
    return this.status === 401 || this.status === 403 || this.status === 503;
  }
}

async function fetchOperationsOverview(): Promise<OperationsOverviewResponse> {
  const result = await operationsOverview();
  if (result.error || !result.data) {
    throw new OperationsOverviewError(result.response?.status);
  }
  return result.data;
}

export function useOperationsOverview() {
  return useQuery({
    queryKey: ["admin", "operations", "overview"],
    queryFn: fetchOperationsOverview,
    refetchInterval: 15_000,
    retry: (failureCount, error) => !(error instanceof OperationsOverviewError && error.isUnavailable) && failureCount < 2,
  });
}
