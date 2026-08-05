import { technologies, type PublicTechnologyLibraryPage } from "@techatlas/api-client";
import { useInfiniteQuery } from "@tanstack/react-query";
import type { TechnologyLibraryState } from "./technology-library-state";

export class TechnologyLibraryError extends Error {
  readonly status: number | undefined;

  constructor(status: number | undefined) {
    super("Technology library is unavailable.");
    this.status = status;
  }
}

async function fetchTechnologyLibrary(
  search: TechnologyLibraryState,
  cursor: string | undefined,
): Promise<PublicTechnologyLibraryPage> {
  const result = await technologies({
    query: {
      ...(search.category ? { category: search.category } : {}),
      ...(search.trend ? { trend: search.trend } : {}),
      ...(cursor ? { cursor } : {}),
    },
  });
  if (result.error || !result.data) {
    throw new TechnologyLibraryError(result.response?.status);
  }
  return result.data;
}

export function useTechnologyLibrary(search: TechnologyLibraryState) {
  return useInfiniteQuery({
    queryKey: ["public", "technology-library", search.category, search.trend],
    queryFn: ({ pageParam }) => fetchTechnologyLibrary(search, pageParam),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  });
}
