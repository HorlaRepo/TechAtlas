import { useAuth0 } from "@auth0/auth0-react";
import { client } from "@techatlas/api-client";
import { type ReactNode, useLayoutEffect } from "react";
import { auth0Configuration } from "./auth0";

export function AuthenticatedApiClient({ children }: { children: ReactNode }) {
  const { getAccessTokenSilently, isAuthenticated } = useAuth0();

  useLayoutEffect(() => {
    client.setConfig({
      baseUrl: "",
      auth: isAuthenticated
        ? () =>
            getAccessTokenSilently({
              authorizationParams: {
                audience: auth0Configuration.audience,
                scope: auth0Configuration.scope,
              },
            })
        : undefined,
    });
  }, [getAccessTokenSilently, isAuthenticated]);

  return <>{children}</>;
}
