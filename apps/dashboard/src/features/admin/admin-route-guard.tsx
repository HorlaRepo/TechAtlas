import { useAuth0 } from "@auth0/auth0-react";
import { Button, Card } from "@techatlas/ui";
import { type ReactNode, useEffect, useRef } from "react";
import { auth0Configuration } from "@/app/auth0";

export function AdminRouteGuard({ children }: { children: ReactNode }) {
  const { error, isAuthenticated, isLoading, loginWithRedirect } = useAuth0();
  const hasStartedLogin = useRef(false);
  const returnTo = `${window.location.pathname}${window.location.search}${window.location.hash}`;
  const browserTestMode = import.meta.env.VITE_E2E_TEST_MODE === "true";

  useEffect(() => {
    if (browserTestMode || isLoading || isAuthenticated || error || hasStartedLogin.current) {
      return;
    }
    hasStartedLogin.current = true;
    void loginWithRedirect({
      appState: { returnTo },
      authorizationParams: {
        audience: auth0Configuration.audience,
        scope: auth0Configuration.scope,
      },
    });
  }, [browserTestMode, error, isAuthenticated, isLoading, loginWithRedirect, returnTo]);

  if (browserTestMode) {
    return <>{children}</>;
  }

  if (isLoading) {
    return <AdminAuthState title="Checking administrator session" description="Loading secure access to the Operations Center." />;
  }

  if (error) {
    return (
      <AdminAuthState
        title="Administrator sign-in unavailable"
        description={error.message}
        action={
          <Button
            onClick={() => {
              hasStartedLogin.current = false;
              void loginWithRedirect({
                appState: { returnTo },
                authorizationParams: { audience: auth0Configuration.audience, scope: auth0Configuration.scope },
              });
            }}
          >
            Try signing in again
          </Button>
        }
      />
    );
  }

  if (!isAuthenticated) {
    return <AdminAuthState title="Redirecting to secure sign-in" description="You need an administrator session to continue." />;
  }

  return <>{children}</>;
}

function AdminAuthState({ title, description, action }: { title: string; description: string; action?: ReactNode }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background p-4 text-on-surface">
      <Card className="flex w-full max-w-xl flex-col items-start gap-4 p-6 sm:p-8" role={action ? "alert" : "status"}>
        <div>
          <p className="font-mono text-xs font-medium tracking-[0.12em] text-primary">ADMINISTRATION</p>
          <h1 className="mt-2 font-sans text-2xl font-semibold">{title}</h1>
          <p className="mt-2 text-sm leading-6 text-on-surface-variant">{description}</p>
        </div>
        {action}
      </Card>
    </main>
  );
}
