import {
  crawlDetail,
  domainChanges,
  domainCrawls,
  domainProfile,
  requestRefresh,
  type PublicChangePage,
  type PublicCrawlDetail,
  type PublicCrawlPage,
  type PublicDomainProfile,
  type PublicRefreshRequest,
} from "@techatlas/api-client";
import { useInfiniteQuery, useMutation, useQuery } from "@tanstack/react-query";

export class DomainProfileError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Domain profile is unavailable.");
    this.status = status;
  }
}

export class DomainHistoryError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Domain history is unavailable.");
    this.status = status;
  }
}

export class RefreshRequestError extends Error {
  readonly status: number | undefined;
  readonly retryAfterSeconds: number | undefined;

  constructor(status: number | undefined, retryAfter: string | null | undefined) {
    super("Refresh request is unavailable.");
    this.status = status;
    const parsed = Number(retryAfter);
    this.retryAfterSeconds = Number.isFinite(parsed) && parsed > 0 ? parsed : undefined;
  }
}

async function fetchDomainProfile(domain: string): Promise<PublicDomainProfile> {
  const result = await domainProfile({ path: { canonical_domain: domain } });
  if (result.error || !result.data) {
    throw new DomainProfileError(result.response?.status);
  }
  return result.data;
}

export function useDomainProfile(domain: string) {
  return useQuery({
    queryKey: ["public", "domain-profile", domain],
    queryFn: () => fetchDomainProfile(domain),
  });
}

async function fetchChanges(domain: string, cursor: string | undefined): Promise<PublicChangePage> {
  const result = await domainChanges({ path: { canonical_domain: domain }, query: { cursor, limit: 20 } });
  if (result.error || !result.data) {
    throw new DomainHistoryError(result.response?.status);
  }
  return result.data;
}

export function useDomainChanges(domain: string) {
  return useInfiniteQuery({
    queryKey: ["public", "domain-history", domain, "changes"],
    queryFn: ({ pageParam }) => fetchChanges(domain, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

async function fetchCrawls(domain: string, cursor: string | undefined): Promise<PublicCrawlPage> {
  const result = await domainCrawls({ path: { canonical_domain: domain }, query: { cursor, limit: 20 } });
  if (result.error || !result.data) {
    throw new DomainHistoryError(result.response?.status);
  }
  return result.data;
}

export function useDomainCrawls(domain: string) {
  return useInfiniteQuery({
    queryKey: ["public", "domain-history", domain, "crawls"],
    queryFn: ({ pageParam }) => fetchCrawls(domain, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}

async function fetchCrawlDetail(domain: string, crawlId: string): Promise<PublicCrawlDetail> {
  const result = await crawlDetail({ path: { canonical_domain: domain, crawl_id: crawlId } });
  if (result.error || !result.data) {
    throw new DomainHistoryError(result.response?.status);
  }
  return result.data;
}

export function useCrawlDetail(domain: string, crawlId: string, enabled: boolean) {
  return useQuery({
    queryKey: ["public", "domain-history", domain, "crawl", crawlId],
    queryFn: () => fetchCrawlDetail(domain, crawlId),
    enabled,
  });
}

async function postRefresh(domain: string): Promise<PublicRefreshRequest> {
  const result = await requestRefresh({ path: { canonical_domain: domain } });
  if (result.error || !result.data) {
    throw new RefreshRequestError(result.response?.status, result.response?.headers.get("retry-after"));
  }
  return result.data;
}

export function useRefreshRequest(domain: string) {
  return useMutation({
    mutationKey: ["public", "domain-refresh", domain],
    mutationFn: () => postRefresh(domain),
  });
}
