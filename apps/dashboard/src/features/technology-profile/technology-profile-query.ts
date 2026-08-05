import { technologyProfile, type PublicTechnologyProfile } from "@techatlas/api-client";
import { useInfiniteQuery } from "@tanstack/react-query";

export class TechnologyProfileError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Technology profile is unavailable.");
    this.status = status;
  }
}

async function fetchTechnologyProfile(technology: string, cursor: string | undefined): Promise<PublicTechnologyProfile> {
  const result = await technologyProfile({
    path: { technology_slug: technology },
    query: cursor ? { cursor } : {},
  });
  if (result.error || !result.data) {
    throw new TechnologyProfileError(result.response?.status);
  }
  return result.data;
}

export function useTechnologyProfile(technology: string) {
  return useInfiniteQuery({
    queryKey: ["public", "technology-profile", technology],
    queryFn: ({ pageParam }) => fetchTechnologyProfile(technology, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (page) => page.domains.next_cursor ?? undefined,
  });
}
