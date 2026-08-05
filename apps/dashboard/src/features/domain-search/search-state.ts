export const searchSorts = ["relevance", "last_crawled_desc", "updated_desc"] as const;

export type DomainSearchSort = (typeof searchSorts)[number];

export type DomainSearchState = {
  q?: string;
  technology?: string[];
  category?: string[];
  country?: string[];
  crawled_since?: number;
  min_confidence?: number;
  limit?: number;
  offset?: number;
  sort?: DomainSearchSort;
};

export function validateDomainSearchState(value: Record<string, unknown>): DomainSearchState {
  const q = optionalString(value.q)?.trim().slice(0, 200);
  const technology = stringList(value.technology);
  const category = stringList(value.category);
  const country = stringList(value.country);
  const crawled_since = boundedInteger(value.crawled_since, 0, Number.MAX_SAFE_INTEGER);
  const min_confidence = boundedInteger(value.min_confidence, 0, 100);
  const limit = boundedInteger(value.limit, 1, 100);
  const offset = boundedInteger(value.offset, 0, 10_000);
  const sort = optionalString(value.sort);

  return {
    ...(q ? { q } : {}),
    ...(technology.length > 0 ? { technology } : {}),
    ...(category.length > 0 ? { category } : {}),
    ...(country.length > 0 ? { country } : {}),
    ...(crawled_since !== undefined ? { crawled_since } : {}),
    ...(min_confidence !== undefined ? { min_confidence } : {}),
    ...(limit !== undefined ? { limit } : {}),
    ...(offset !== undefined ? { offset } : {}),
    ...(isSearchSort(sort) ? { sort } : {}),
  };
}

export function stringifySearchParams(search: Record<string, unknown>): string {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(search)) {
    if (value === undefined || value === null || value === "") {
      continue;
    }
    if (Array.isArray(value)) {
      for (const item of value) {
        if (typeof item === "string" && item.length > 0) {
          params.append(key, item);
        }
      }
      continue;
    }
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      params.set(key, String(value));
    }
  }
  return params.toString();
}

export function stringifyRouterSearch(search: Record<string, unknown>): string {
  const query = stringifySearchParams(search);
  return query ? `?${query}` : "";
}

export function parseSearchParams(search: string): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  const params = new URLSearchParams(search.startsWith("?") ? search.slice(1) : search);
  for (const [key, value] of params) {
    const existing = result[key];
    if (existing === undefined) {
      result[key] = value;
    } else if (Array.isArray(existing)) {
      existing.push(value);
    } else {
      result[key] = [existing, value];
    }
  }
  return result;
}

export function searchWithOffset(search: DomainSearchState, offset: number): DomainSearchState {
  return validateDomainSearchState({ ...search, ...(offset > 0 ? { offset } : { offset: undefined }) });
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function stringList(value: unknown): string[] {
  const values = Array.isArray(value) ? value : value === undefined ? [] : [value];
  return [...new Set(values.filter((item): item is string => typeof item === "string").map((item) => item.trim()).filter(Boolean))];
}

function boundedInteger(value: unknown, minimum: number, maximum: number): number | undefined {
  const candidate = typeof value === "number" ? value : typeof value === "string" && value.trim().length > 0 ? Number(value) : Number.NaN;
  return Number.isInteger(candidate) && candidate >= minimum && candidate <= maximum ? candidate : undefined;
}

function isSearchSort(value: string | undefined): value is DomainSearchSort {
  return value !== undefined && searchSorts.some((sort) => sort === value);
}
