import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AdminRouteGuard } from "./admin-route-guard";

const authState = vi.hoisted(() => ({
  error: undefined as Error | undefined,
  isAuthenticated: false,
  isLoading: false,
  loginWithRedirect: vi.fn(),
}));

vi.mock("@auth0/auth0-react", () => ({
  useAuth0: () => authState,
}));

vi.mock("@/app/auth0", () => ({
  auth0Configuration: {
    audience: "https://api.techatlas",
    clientId: "test-client-id",
    domain: "example.us.auth0.com",
    scope: "openid profile email admin:read admin:operate",
  },
}));

describe("AdminRouteGuard", () => {
  beforeEach(() => {
    authState.error = undefined;
    authState.isAuthenticated = false;
    authState.isLoading = false;
    authState.loginWithRedirect.mockReset();
    window.history.replaceState({}, "", "/admin/domains?status=active");
  });

  it("redirects an unauthenticated visitor to Auth0 and preserves the requested admin URL", async () => {
    render(<AdminRouteGuard><p>Protected content</p></AdminRouteGuard>);

    expect(screen.getByRole("status")).toHaveTextContent("Redirecting to secure sign-in");
    await waitFor(() => {
      expect(authState.loginWithRedirect).toHaveBeenCalledWith({
        appState: { returnTo: "/admin/domains?status=active" },
        authorizationParams: {
          audience: "https://api.techatlas",
          scope: "openid profile email admin:read admin:operate",
        },
      });
    });
  });

  it("renders the protected route only for an authenticated administrator", () => {
    authState.isAuthenticated = true;

    render(<AdminRouteGuard><p>Protected content</p></AdminRouteGuard>);

    expect(screen.getByText("Protected content")).toBeInTheDocument();
    expect(authState.loginWithRedirect).not.toHaveBeenCalled();
  });
});
