export type ComparisonState = {
  domain?: string[];
};

const maximumDomains = 10;

export function validateComparisonState(value: Record<string, unknown>): ComparisonState {
  const rawDomains = Array.isArray(value.domain) ? value.domain : value.domain === undefined ? [] : [value.domain];
  const domains = [...new Set(rawDomains
    .filter((domain): domain is string => typeof domain === "string")
    .map(normalizeComparisonDomain)
    .filter((domain): domain is string => domain !== undefined))]
    .slice(0, maximumDomains);
  return domains.length > 0 ? { domain: domains } : {};
}

export function addComparisonDomain(search: ComparisonState, domain: string): ComparisonState {
  return validateComparisonState({ domain: [...(search.domain ?? []), domain] });
}

export function removeComparisonDomain(search: ComparisonState, domain: string): ComparisonState {
  return validateComparisonState({ domain: (search.domain ?? []).filter((value) => value !== domain) });
}

export function normalizeComparisonDomain(value: unknown): string | undefined {
  if (typeof value !== "string" || value.trim().length === 0) {
    return undefined;
  }
  const candidate = value.trim();
  try {
    const parsed = new URL(candidate.includes("://") ? candidate : `https://${candidate}`);
    if (!parsed.hostname || parsed.username || parsed.password) {
      return undefined;
    }
    return parsed.hostname.toLowerCase();
  } catch {
    return undefined;
  }
}

export function canCompare(search: ComparisonState): boolean {
  const count = search.domain?.length ?? 0;
  return count >= 2 && count <= maximumDomains;
}
