# Dashboard browser checks

The browser suite uses deterministic API fixtures and `VITE_E2E_TEST_MODE=true`.
That flag bypasses only the dashboard's client-side admin route guard so the
admin shell can be checked without an Auth0 tenant; the production build and
runtime behaviour still require normal Auth0 authentication.

Run the suite with `pnpm --filter @techatlas/dashboard test:e2e`. The tracked
screenshots under `e2e/visual.spec.ts-snapshots` are the reviewed desktop,
tablet, and mobile baselines. Update them only after visual review with
`pnpm --filter @techatlas/dashboard test:e2e:update`.
