export const technologyTrends = ["growing", "declining", "stable"] as const;
export const technologyLibraryViews = ["grid", "list"] as const;

export type TechnologyLibraryTrend = (typeof technologyTrends)[number];
export type TechnologyLibraryView = (typeof technologyLibraryViews)[number];

export type TechnologyLibraryState = {
  category?: string;
  trend?: TechnologyLibraryTrend;
  view?: "list";
};

export function validateTechnologyLibraryState(value: Record<string, unknown>): TechnologyLibraryState {
  const category = optionalString(value.category);
  const trend = optionalString(value.trend);
  const view = optionalString(value.view);

  return {
    ...(category && isSlug(category) ? { category } : {}),
    ...(isTechnologyTrend(trend) ? { trend } : {}),
    ...(view === "list" ? { view } : {}),
  };
}

export function currentTechnologyLibraryView(search: TechnologyLibraryState): TechnologyLibraryView {
  return search.view ?? "grid";
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" ? value.trim() : undefined;
}

function isSlug(value: string): boolean {
  return value.length > 0 && value.length <= 64 && /^[a-z0-9-]+$/.test(value);
}

function isTechnologyTrend(value: string | undefined): value is TechnologyLibraryTrend {
  return value !== undefined && technologyTrends.some((trend) => trend === value);
}
