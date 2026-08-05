import { describe, expect, it } from "vitest";
import { configuredObservabilityUrl } from "./observability";

describe("configuredObservabilityUrl", () => {
  it("accepts credential-free HTTP(S) links", () => {
    expect(configuredObservabilityUrl("https://grafana.example.test/ops")).toBe("https://grafana.example.test/ops");
    expect(configuredObservabilityUrl(" http://localhost:3003 ")).toBe("http://localhost:3003/");
  });

  it("rejects absent, non-HTTP, and credential-bearing links", () => {
    expect(configuredObservabilityUrl()).toBeUndefined();
    expect(configuredObservabilityUrl(" ")).toBeUndefined();
    expect(configuredObservabilityUrl("ftp://grafana.example.test")).toBeUndefined();
    expect(configuredObservabilityUrl("https://operator:secret@grafana.example.test")).toBeUndefined();
  });
});
