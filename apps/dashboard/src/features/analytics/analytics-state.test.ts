import { describe, expect, it } from "vitest";
import { analyticsDays, analyticsSelectedTechnology, validateAnalyticsState } from "./analytics-state";

describe("analytics URL state", () => {
  it("uses a bounded default history window", () => {
    expect(analyticsDays(validateAnalyticsState({}))).toBe(30);
    expect(analyticsDays(validateAnalyticsState({ since_days: "90" }))).toBe(90);
    expect(analyticsDays(validateAnalyticsState({ since_days: "31" }))).toBe(30);
  });

  it("retains only valid selected technology slugs", () => {
    expect(analyticsSelectedTechnology(validateAnalyticsState({ technology: "nextjs" }))).toBe("nextjs");
    expect(analyticsSelectedTechnology(validateAnalyticsState({ technology: "Next JS" }))).toBeUndefined();
  });
});
