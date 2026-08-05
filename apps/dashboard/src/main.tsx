import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Auth0Provider } from "@auth0/auth0-react";
import "./app/styles.css";
import { App } from "./app/app";
import { auth0Configuration } from "./app/auth0";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Dashboard root element was not found.");
}

createRoot(rootElement).render(
  <StrictMode>
    <Auth0Provider
      domain={auth0Configuration.domain}
      clientId={auth0Configuration.clientId}
      authorizationParams={{
        audience: auth0Configuration.audience,
        redirect_uri: window.location.origin,
        scope: auth0Configuration.scope,
      }}
    >
      <App />
    </Auth0Provider>
  </StrictMode>,
);
