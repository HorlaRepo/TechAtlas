# Dashboard performance budget

The production dashboard budget is measured after `pnpm build` against gzip-compressed assets:

| Asset class | Budget |
| --- | ---: |
| All JavaScript | 250 KB |
| All CSS | 50 KB |

Run `pnpm performance:check` after a production build. CI enforces the same check. New route code should remain lazy-loadable when it would otherwise materially increase the initial dashboard payload.
