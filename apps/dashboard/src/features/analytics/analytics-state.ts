export type AnalyticsState = { since_days?: 30 | 90 | 365; technology?: string };

export function validateAnalyticsState(value: Record<string, unknown>): AnalyticsState {
  const days = Number(value.since_days);
  const technology = typeof value.technology === "string" && validSlug(value.technology) ? value.technology : undefined;
  return { ...(days === 90 || days === 365 ? { since_days: days } : {}), ...(technology ? { technology } : {}) };
}

export function analyticsSelectedTechnology(state: AnalyticsState): string | undefined {
  return state.technology;
}

function validSlug(value: string): boolean {
  return value.length > 0 && value.length <= 64 && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}

export function analyticsDays(state: AnalyticsState): 30 | 90 | 365 {
  return state.since_days ?? 30;
}
