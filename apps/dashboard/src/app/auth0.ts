const ADMIN_API_SCOPE = "openid profile email admin:read admin:operate";

function requiredEnvironmentValue(name: "VITE_AUTH0_DOMAIN" | "VITE_AUTH0_CLIENT_ID" | "VITE_AUTH0_AUDIENCE") {
  const value = import.meta.env[name]?.trim();
  if (!value) {
    throw new Error(`${name} must be configured before starting the dashboard.`);
  }
  return value;
}

export const auth0Configuration = {
  domain: requiredEnvironmentValue("VITE_AUTH0_DOMAIN"),
  clientId: requiredEnvironmentValue("VITE_AUTH0_CLIENT_ID"),
  audience: requiredEnvironmentValue("VITE_AUTH0_AUDIENCE"),
  scope: ADMIN_API_SCOPE,
};
