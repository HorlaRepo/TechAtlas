const adminBreadcrumbs: Record<string, string> = {
  "/admin/overview": "Overview",
  "/admin/analytics": "Operational Analytics",
  "/admin/monitoring": "Monitoring",
  "/admin/settings": "Settings",
  "/admin/domains": "Domains",
  "/admin/imports": "Imports",
  "/admin/audit": "Audit",
  "/admin/scheduler": "Scheduler",
  "/admin/queue": "Queue",
  "/admin/workers": "Workers",
  "/admin/rules": "Detection Rules",
};

export function adminBreadcrumbForPath(pathname: string): string {
  return adminBreadcrumbs[pathname] ?? "Operations";
}
