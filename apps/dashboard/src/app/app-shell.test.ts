import { describe, expect, it } from "vitest";
import { adminBreadcrumbForPath } from "./admin-breadcrumbs";

describe("adminBreadcrumbForPath", () => {
  it("returns the active protected route label", () => {
    expect(adminBreadcrumbForPath("/admin/domains")).toBe("Domains");
    expect(adminBreadcrumbForPath("/admin/rules")).toBe("Detection Rules");
    expect(adminBreadcrumbForPath("/admin/scheduler")).toBe("Scheduler");
    expect(adminBreadcrumbForPath("/admin/analytics")).toBe("Operational Analytics");
    expect(adminBreadcrumbForPath("/admin/monitoring")).toBe("Monitoring");
    expect(adminBreadcrumbForPath("/admin/settings")).toBe("Settings");
  });

  it("uses an operations fallback outside known protected routes", () => {
    expect(adminBreadcrumbForPath("/admin/unknown")).toBe("Operations");
  });
});
