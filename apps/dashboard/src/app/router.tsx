import { Outlet, createRootRoute, createRoute, createRouter } from "@tanstack/react-router";
import { PublicAboutRoute } from "./public-about-route";
import { PublicCompareRoute } from "./public-compare-route";
import { PublicAnalyticsRoute } from "./public-analytics-route";
import { PublicDomainsRoute } from "./public-domains-route";
import { PublicProviderRoute } from "./public-provider-route";
import { PublicDomainRoute } from "./public-domain-route";
import { PublicHomeRoute } from "./public-home-route";
import { PublicSearchRoute } from "./public-search-route";
import { PublicTechnologyRoute } from "./public-technology-route";
import { PublicTechnologiesRoute } from "./public-technologies-route";
import { OperationsOverview } from "@/features/operations/operations-overview";
import { AdminDomainsPage } from "@/features/admin/admin-domains-page";
import { AdminImportsPage } from "@/features/admin/admin-imports-page";
import { AdminAuditPage } from "@/features/admin/admin-audit-page";
import { AdminRuntimePage } from "@/features/admin/admin-runtime-page";
import { AdminRulesPage } from "@/features/admin/admin-rules-page";
import { AdminMonitoringPage } from "@/features/admin/admin-monitoring-page";
import { AdminOperationalAnalyticsPage } from "@/features/admin/admin-operational-analytics-page";
import { AdminRouteLayout } from "@/features/admin/admin-route-layout";
import { AdminSettingsPage } from "@/features/admin/admin-settings-page";
import { RouteErrorFallback } from "./error-boundary";
import { parseSearchParams, stringifyRouterSearch, validateDomainSearchState } from "@/features/domain-search/search-state";
import { validateTechnologyLibraryState } from "@/features/technology-library/technology-library-state";
import { validateComparisonState } from "@/features/domain-comparison/comparison-state";
import { validateAnalyticsState } from "@/features/analytics/analytics-state";

const rootRoute = createRootRoute({
  component: Outlet,
  errorComponent: RouteErrorFallback,
});


const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  component: PublicHomeRoute,
});

const overviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/admin/overview",
  component: () => <AdminRouteLayout><OperationsOverview /></AdminRouteLayout>,
});
const adminAnalyticsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/analytics", component: () => <AdminRouteLayout><AdminOperationalAnalyticsPage /></AdminRouteLayout> });
const adminMonitoringRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/monitoring", component: () => <AdminRouteLayout><AdminMonitoringPage /></AdminRouteLayout> });
const adminSettingsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/settings", component: () => <AdminRouteLayout><AdminSettingsPage /></AdminRouteLayout> });
const adminDomainsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/domains", component: () => <AdminRouteLayout><AdminDomainsPage /></AdminRouteLayout> });
const adminImportsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/imports", component: () => <AdminRouteLayout><AdminImportsPage /></AdminRouteLayout> });
const adminAuditRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/audit", component: () => <AdminRouteLayout><AdminAuditPage /></AdminRouteLayout> });
const adminSchedulerRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/scheduler", component: () => <AdminRouteLayout><AdminRuntimePage kind="scheduler" /></AdminRouteLayout> });
const adminQueueRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/queue", component: () => <AdminRouteLayout><AdminRuntimePage kind="queue" /></AdminRouteLayout> });
const adminWorkersRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/workers", component: () => <AdminRouteLayout><AdminRuntimePage kind="workers" /></AdminRouteLayout> });
const adminRulesRoute = createRoute({ getParentRoute: () => rootRoute, path: "/admin/rules", component: () => <AdminRouteLayout><AdminRulesPage /></AdminRouteLayout> });

const searchRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/search",
  validateSearch: validateDomainSearchState,
  component: PublicSearchRoute,
});

const domainsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains",
  validateSearch: validateDomainSearchState,
  component: PublicDomainsRoute,
});

const aboutRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/about",
  component: PublicAboutRoute,
});

const domainProfileRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/domains/$domain",
  component: PublicDomainRoute,
});

const comparisonRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/compare",
  validateSearch: validateComparisonState,
  component: PublicCompareRoute,
});
const analyticsRoute = createRoute({ getParentRoute: () => rootRoute, path: "/analytics", validateSearch: validateAnalyticsState, component: PublicAnalyticsRoute });
const providerRoute = createRoute({ getParentRoute: () => rootRoute, path: "/providers/$provider", component: PublicProviderRoute });

const technologiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/technologies",
  validateSearch: validateTechnologyLibraryState,
  component: PublicTechnologiesRoute,
});

const technologyProfileRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/technologies/$technology",
  component: PublicTechnologyRoute,
});

const routeTree = rootRoute.addChildren([indexRoute, overviewRoute, adminAnalyticsRoute, adminMonitoringRoute, adminSettingsRoute, adminDomainsRoute, adminImportsRoute, adminAuditRoute, adminSchedulerRoute, adminQueueRoute, adminWorkersRoute, adminRulesRoute, searchRoute, domainsRoute, domainProfileRoute, comparisonRoute, analyticsRoute, aboutRoute, providerRoute, technologiesRoute, technologyProfileRoute]);

export const router = createRouter({ routeTree, parseSearch: parseSearchParams, stringifySearch: stringifyRouterSearch });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
