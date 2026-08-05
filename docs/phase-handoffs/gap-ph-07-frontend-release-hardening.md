# GAP-PH-07 frontend release hardening

**Completed:** 2026-08-05

## Delivered outcome

- Added Playwright and axe-core route checks for public routes, the protected
  admin shell, quick-crawl dialog, filters, charts, responsive navigation, and
  reduced-motion rendering.
- Added deterministic API fixtures and a `VITE_E2E_TEST_MODE` route-guard
  bypass. The bypass exists only in the browser-test bundle and does not alter
  production Auth0 authentication or API authorization.
- Added reviewed Git-tracked screenshots for the home, dense domain search,
  domain profile, populated and empty analytics charts, and admin overview at
  desktop, tablet, and mobile widths.
- Made horizontally scrollable analytics tables keyboard-focusable and restore
  focus when the quick-crawl dialog or compact navigation closes.
- Added a macOS CI browser job because the checked-in Playwright baselines are
  macOS Chromium images.

## Performance budget

`scripts/check-dashboard-budget.mjs` reads the Vite manifest and checks:

| Asset class | Budget | Recorded gzip size |
| --- | ---: | ---: |
| Initial JavaScript | 250 KB | 225,888 bytes |
| CSS | 50 KB | 7,459 bytes |
| Each lazy JavaScript chunk | 400 KB | 373,221 bytes (ECharts) |

The ECharts chunk remains lazy-loaded and is within the approved 400 KB lazy
chunk budget. No exception is required.

## Verification

- `pnpm --filter @techatlas/dashboard typecheck`
- `pnpm --filter @techatlas/dashboard lint`
- `pnpm --filter @techatlas/dashboard build`
- `node scripts/check-dashboard-budget.mjs`
- `pnpm --filter @techatlas/dashboard test:e2e`

The browser test uses Chromium, deterministic fixtures, and `reducedMotion:
"reduce"`. Update the reviewed visual baselines with
`pnpm --filter @techatlas/dashboard test:e2e:update`.
