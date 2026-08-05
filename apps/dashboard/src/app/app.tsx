import { RouterProvider } from "@tanstack/react-router";
import { AuthenticatedApiClient } from "./authenticated-api-client";
import { AppProviders } from "./providers";
import { router } from "./router";
import { DashboardErrorBoundary } from "./error-boundary";

export function App() {
  return (
    <AppProviders>
      <AuthenticatedApiClient>
        <DashboardErrorBoundary><RouterProvider router={router} /></DashboardErrorBoundary>
      </AuthenticatedApiClient>
    </AppProviders>
  );
}
