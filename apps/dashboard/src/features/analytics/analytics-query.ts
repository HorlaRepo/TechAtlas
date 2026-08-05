import { analyticsAdoptionHistory, analyticsChanges, analyticsDiscovery, analyticsMovers, analyticsOverview, analyticsRankings } from "@techatlas/api-client";
import { useQuery } from "@tanstack/react-query";

class AnalyticsError extends Error {}
async function read<T>(request: Promise<{ data?: T; error?: unknown }>): Promise<T> { const result = await request; if (result.error || !result.data) throw new AnalyticsError(); return result.data; }
export function useAnalytics(days: number, technology?: string) {
  return {
    overview: useQuery({ queryKey: ["public", "analytics", "overview"], queryFn: () => read(analyticsOverview()) }),
    rankings: useQuery({ queryKey: ["public", "analytics", "rankings"], queryFn: () => read(analyticsRankings()) }),
    movers: useQuery({ queryKey: ["public", "analytics", "movers", days], queryFn: () => read(analyticsMovers({ query: { since_days: days } })) }),
    changes: useQuery({ queryKey: ["public", "analytics", "changes", days], queryFn: () => read(analyticsChanges({ query: { since_days: days } })) }),
    history: useQuery({ queryKey: ["public", "analytics", "adoption-history", days, technology], queryFn: () => read(analyticsAdoptionHistory({ query: { since_days: days, ...(technology ? { technology } : {}) } })) }),
    discovery: useQuery({ queryKey: ["public", "analytics", "discovery", days], queryFn: () => read(analyticsDiscovery({ query: { since_days: days } })) }),
  };
}
