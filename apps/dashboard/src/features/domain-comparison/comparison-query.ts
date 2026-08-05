import { compare, searchDomains, type PublicComparisonResponse, type PublicDomainSearchPage } from "@techatlas/api-client";
import { useQuery } from "@tanstack/react-query";
import type { ComparisonState } from "./comparison-state";

export class ComparisonError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Domain comparison is unavailable.");
    this.status = status;
  }
}

async function fetchComparison(domains: string[]): Promise<PublicComparisonResponse> {
  const result = await compare({ query: { domain: domains } });
  if (result.error || !result.data) {
    throw new ComparisonError(result.response?.status);
  }
  return result.data;
}

export function useDomainComparison(search: ComparisonState) {
  const domains = search.domain ?? [];
  return useQuery({
    queryKey: ["public", "domain-comparison", domains],
    queryFn: () => fetchComparison(domains),
    enabled: domains.length >= 2,
  });
}

async function fetchDomainSuggestions(query: string): Promise<PublicDomainSearchPage> {
  const result = await searchDomains({ query: { q: query, limit: 5 } });
  if (result.error || !result.data) {
    throw new ComparisonError(result.response?.status);
  }
  return result.data;
}

export function useComparisonDomainSuggestions(query: string) {
  const normalizedQuery = query.trim();
  return useQuery({
    queryKey: ["public", "comparison-domain-suggestions", normalizedQuery],
    queryFn: () => fetchDomainSuggestions(normalizedQuery),
    enabled: normalizedQuery.length >= 2,
  });
}
