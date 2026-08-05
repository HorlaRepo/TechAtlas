# OpenAPI contract

This directory contains the generated, validated API contract. Generate it from the Rust API implementation and then generate `packages/api-client`; neither artifact is manually edited.

From the repository root:

```sh
pnpm api:openapi
pnpm api:client:generate
pnpm api:openapi:check
pnpm api:client:check
```
