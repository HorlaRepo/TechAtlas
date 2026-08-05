# Auth0 administrator access

Configure the API with `ADMIN_OIDC_ISSUER`, `ADMIN_OIDC_AUDIENCE`, and
`ADMIN_OIDC_JWKS_URL`. Use HTTPS endpoints for production and give the API network access to the
JWKS endpoint. Set `ADMIN_RATE_LIMIT_PER_MINUTE` and
`ADMIN_MUTATION_RATE_LIMIT_PER_MINUTE` for the expected operational workload.

In Auth0, configure the TechAtlas Dashboard Single Page Application with the following allowed
callback, logout, web-origin, and CORS URLs:

- `http://localhost:5173`
- `https://techatlas.dpdns.org`

The production dashboard build must receive `VITE_AUTH0_DOMAIN`, `VITE_AUTH0_CLIENT_ID`, and
`VITE_AUTH0_AUDIENCE`. For the current tenant, use
`dev-6cezdkyjkqhpt8ak.us.auth0.com`, `0vvQlrj9ZReU7f4Z2KwF1ypihzL96Zh2`, and
`https://api.techatlas` respectively. Configure the API process with issuer
`https://dev-6cezdkyjkqhpt8ak.us.auth0.com/` and JWKS URL
`https://dev-6cezdkyjkqhpt8ak.us.auth0.com/.well-known/jwks.json`.

Configure the Auth0 API with identifier `https://api.techatlas`, enable RBAC and "Add Permissions
in the Access Token", then define `admin:read` and `admin:operate`. Assign `admin_viewer` the
read permission and `admin_operator` both permissions. Auth0 must issue RS256 access tokens with
a non-empty `sub`, the configured `iss` and `aud`, and a `permissions` array. Do not send browser
ID tokens to the API; use access tokens intended for this audience.

For key rotation, publish the new key in the JWKS before issuing tokens that reference its `kid`.
The API refreshes its cache when it sees an unknown key and caches known keys for five minutes.
Remove a compromised key from the JWKS and revoke or shorten the affected access tokens at the
identity provider. Audit events identify the authenticated `sub`; do not use shared service
subjects for human administrator actions.
