import { searchDomains, type PublicDomainSearchPage } from "@techatlas/api-client";
import { useQuery } from "@tanstack/react-query";
import type { DomainSearchState } from "./search-state";

export class DomainSearchError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Domain search is unavailable.");
    this.status = status;
  }
}

async function fetchDomainSearch(search: DomainSearchState): Promise<PublicDomainSearchPage> {
  const result = await searchDomains({ query: search });
  if (result.error || !result.data) {
    throw new DomainSearchError(result.response?.status);
  }
  return result.data;
}

export function useDomainSearch(search: DomainSearchState) {
  return useQuery({
    queryKey: ["public", "domain-search", search],
    queryFn: () => fetchDomainSearch(search),
  });
}
