# TechAtlas Improvement Plan — Crawl Reliability & Detection Coverage

**Status:** v1 complete, addressing production data quality issues
**Owner:** Francis Oladosu
**Prepared for:** Codex implementation
**Basis:** Dead-letter/failure-reason data pulled 2026-08-05 (528 failed attempts, 134 domains, 24h window), cross-referenced against a second independent roadmap review

---

## 0. Principle for this phase

Every item below is prioritized by **measured impact on the current failure data**, not by novelty. Differentiation features (competitor-relationship graphs, public API, alerts) are real and worth building, but they are explicitly deferred — see Section 5 — because they compound on top of crawl/detection quality rather than substituting for it. Ship Sections 1–3 first; re-pull the failure-reason breakdown before starting Section 4.

---

## 1. Priority 0 — Robots.txt handling (fixes 63.1% of failures / 85 of 134 stuck domains)

Current behavior treats "robots.txt unavailable" as one undifferentiated terminal state. It is actually five distinct failure modes requiring different handling. This is the single highest-leverage fix available — implement and re-measure before anything else.

### 1.1 Lenient robots.txt parsing (recovers ~80 attempts)

- **Problem:** Parser currently aborts the entire robots.txt on first malformed/unparseable line, causing the whole file to be treated as unavailable.
- **Fix:** Per RFC 9309, skip unparseable lines individually and apply whatever directives parse successfully. Do not fail the file on partial malformation.
- **Tasks:**
  - Audit current robots.txt parser for abort-on-error behavior vs. line-skip behavior.
  - Add tolerance for: BOM markers, non-UTF-8 byte sequences (attempt best-effort decode, fall back to Latin-1 before giving up), non-standard directives (`Clean-param`, `Host`, `Crawl-delay` variants), and comment placement edge cases.
  - Add a test corpus of malformed robots.txt files (pull actual failing examples from the 80 affected domains) and assert partial-parse success on each.
  - Log a `robots_partial_parse` event (non-failure, informational) when lines are skipped, so this doesn't silently mask genuinely broken policy files going forward.

### 1.2 Differentiate retry-eligible vs. terminal robots.txt fetch failures (recovers ~43 attempts: 23 timeouts + ~20 HTTP 400s, unsticks domains like php.net's 504)

- **Problem:** Timeouts, 5xx, and 400-class responses on the robots.txt fetch itself are currently landing in the same terminal bucket as a genuine policy fetch failure.
- **Fix:** Split `robots_unavailable` into two sub-states:
  - `robots_transient` — timeout, 5xx, connection reset, DNS SERVFAIL → retry-eligible, goes back into the scheduler with exponential backoff (suggest: 3 attempts, base 5min, cap 6h).
  - `robots_permanent` — repeated failure after retry exhaustion, or unparseable response body after successful fetch → terminal, defaults to full-disallow per spec.
- **Tasks:**
  - Add `robots_transient` and `robots_permanent` as distinct dead-letter/state values (keep `robots_unavailable` as a deprecated alias if needed for backward compatibility in dashboards).
  - Route HTTP 400 on robots.txt fetch through the same retry path as timeouts — a 400 on a static file fetch is very likely a malformed request on our side (bad header/encoding), not a durable site policy signal. Add a diagnostic log of the actual request sent when a 400 occurs, to confirm root cause before assuming it's site-side.
  - Wire retry attempts into existing scheduler/queue infrastructure (Redis Streams) rather than building a parallel retry mechanism.
  - Add regression test: simulate a domain returning 504 on first robots.txt fetch attempt and 200 on second; assert it does not enter terminal state after the first failure.

### 1.3 Audit HTTP 403 on robots.txt fetch (52 attempts — may be correct behavior, verify first)

- **Problem:** Per spec, 403/401 on robots.txt fetch should default to full-disallow, and current behavior may already be correct. But a subset of these could be our own crawler being bot-filtered on the robots.txt request specifically.
- **Tasks:**
  - Pull the request headers/User-Agent sent for the 52 affected attempts.
  - Cross-reference against known WAF/bot-defense signatures (Cloudflare, Akamai challenge pages return distinct 403 body content — check if response bodies match a challenge page vs. an actual `403 Forbidden` from the origin).
  - If a meaningful share are challenge-page 403s: this is a UA/fingerprint problem, not a parsing problem — file as a separate task, do not conflate with 1.1/1.2.
  - If confirmed genuine 403 (origin explicitly denying), no code change needed — document as expected behavior in the failure-reason dashboard tooltip so it's not mistaken for a bug later.

**Section 1 acceptance criteria:** Re-run the failure-reason report after deployment. Target: `robots_unavailable`-class terminal failures drop from 80 to near-zero (excluding genuine 403 policy denials), and previously-stuck domains (php.net and similar) successfully complete a crawl within one scheduler cycle after the fix.

---

## 2. Priority 1 — SSRF/unsafe_target false-positive audit (17.4% of failures, 23 domains)

- **Problem:** Unknown whether `unsafe_target` rejections are correctly identifying unsafe domains or false-positiving on legitimate sites with unusual DNS/CDN topology.
- **Tasks:**
  - Pull the list of all 23 domains currently in `unsafe_target` terminal state.
  - Manually resolve each domain's full A/AAAA record set and classify: (a) genuinely resolves to private/reserved/loopback range, (b) resolves to a CDN edge IP that a naive private-range check might misclassify, (c) multi-A-record domain where only one record trips the check.
  - **Do not weaken SSRF protection blindly.** If any of the 23 are genuine attempts to resolve to internal ranges, keep the rejection. Only fix the validator logic if it's provably rejecting legitimate public targets.
  - Likely fix if false positives are confirmed: validate SSRF safety per-resolved-IP for the *specific* IP the crawler will actually connect to (post DNS resolution, immediately before connect — to also close the classic DNS-rebinding TOCTOU gap), rather than rejecting the whole domain if any record in the full result set looks private.
  - Add test cases for multi-record domains and CDN edge IP ranges once root cause is confirmed.

---

## 3. Priority 2 — Detection engine coverage expansion

Current rule corpus is under-covering relative to what's structurally detectable. Split into signal-source expansion (what we look at) and confidence modeling (how we weigh what we find) — these are separable workstreams.

### 3.1 New signal sources (deterministic, no rendering required)

Add extraction + rule-matching support for, in priority order (cheapest/highest-signal first):

1. **Favicon hashing** — MD5 or perceptual hash of `favicon.ico`, matched against known-technology favicon hash table. Works even without JS execution. Flag: hash collisions across unrelated technologies are possible — treat as medium-confidence signal, not sole basis for detection.
2. **Cookie name/pattern matching** — extract `Set-Cookie` headers, match against known platform cookie name patterns (e.g. `_shopify_s`, `wordpress_logged_in_*`, `_ga`, `_hjid`).
3. **DNS-based signals** — CNAME pattern matching (`*.myshopify.com`, `*.webflow.io`, `*.vercel-dns.com`, etc.), MX record matching for email platform detection, TXT/SPF record matching for marketing tool verification records.
4. **Security headers / CSP directives** — `Content-Security-Policy`, `Strict-Transport-Security`, `X-Powered-By` (still present on a surprising number of sites), `Server` header — already partially covered per current architecture, audit for completeness.
5. **JSON-LD structured data** — parse `<script type="application/ld+json">` blocks for platform-identifying schema.org markers.
6. **Static-HTML-visible JS globals** — where present without execution: inline `<script>` block content matching for `window.Shopify`, `dataLayer`, `ga(`/`gtag(` calls that appear in raw HTML (many analytics snippets are inlined, not loaded async, so this doesn't require rendering).
7. **Webpack/build-tool artifact fingerprints** — chunk filename patterns, source map references (when public), manifest.json / Web App Manifest content.

### 3.2 Rule corpus size audit

- Pull current rule count by category, compare against scope of a reference open technology-detection dataset (verify licensing terms before importing any external rule definitions wholesale — do not copy proprietary rule sets without checking license compatibility with an open-source release).
- If current corpus is in the low hundreds, corpus size — not detection logic — is likely the primary bottleneck for "few technologies detected." Prioritize rule authoring throughput over further signal-source expansion once 3.1 ships.

### 3.3 Confidence scoring (replace binary match with weighted evidence)

- **Problem:** If current rules require AND-logic across multiple signals to confirm a technology, any single missing/lazy-loaded/stripped signal causes a full miss.
- **Fix:** Move to weighted evidence scoring per detection:
  - Suggested starting weights (tune empirically): strong runtime/DOM signal (e.g. `window.__NEXT_DATA__`) = 100, response header match = 90, script URL pattern = 60, cookie match = 50, favicon hash = 40, CSS class fingerprint = 20.
  - Define confidence tiers (e.g. high ≥80, medium 40–79, low <40) and surface the tier in domain profile evidence view, not just a binary "detected."
  - Preserve full evidence trail per detection (which signals contributed, at what weight) — this is additive to the existing immutable snapshot/evidence model, not a replacement.
- This is independent of 3.1 and can ship in parallel once the current rule format supports multi-signal aggregation (audit whether it already does before scoping as new work).

---

## 4. Priority 3 — Selective JavaScript rendering tier

This directly addresses the largest *structural* blind spot: any technology whose only signal exists post-render (React/Vue/Next/Nuxt hydration state, `window.__NEXT_DATA__`, `window.__NUXT__`, `__APOLLO_STATE__`, GraphQL endpoint discovery, Service Worker registration) is currently undetectable regardless of rule quality, because the pipeline never executes JS.

- **Architecture:** Separate worker service (does not need to be Rust — a small Playwright/Chromium pool in Node/TypeScript is reasonable given the stack already spans both languages), dispatched to selectively, not universally.
- **Selection logic (escalation pipeline):**
  1. Run static crawl first (current pipeline, unchanged).
  2. Evaluate "evidence sufficiency" heuristic post-static-crawl: e.g., detected technology count below a threshold, AND/OR response body is suspiciously small relative to a typical SPA shell (e.g. under ~2KB of meaningful content, high ratio of `<script>` tags to visible text), AND/OR known SPA-framework marker present without full evidence (e.g. a `<div id="root">` or `<div id="__next">` with no further signal).
  3. If insufficient: escalate to headless render, capture rendered DOM + runtime global snapshot, re-run detection rules against rendered output, merge evidence.
- **Resource constraints (critical given current 2 vCPU / 12GB deployment target):** Cap concurrent browser contexts hard (start at 2–3), set aggressive per-page timeout (10–15s), and treat this tier as sampled/selective, not default-on for all domains. Document this constraint explicitly in the admin ops dashboard so reviewers understand why render-tier throughput is capped.
- **Do not build this before Sections 1–2 ship.** It's the most expensive fix per domain recovered; the robots.txt and signal-expansion work recovers more failures per engineering hour.

---

## 5. Deferred — differentiation features (explicitly out of scope for this phase)

These are good ideas and worth a follow-up plan once Sections 1–4 are shipped and re-measured, but they build on top of crawl/detection quality rather than fixing it. Listed here so they aren't lost, not because they're low-value:

- Technology relationship graph (co-occurrence: "X% of Next.js sites also use React")
- Technology compatibility/common-stack surfacing
- AI-assisted rule discovery (surface frequently-seen unmatched patterns for human review — genuinely reasonable use of AI here since it's suggest-only, human-approved, and doesn't replace deterministic detection)
- Public REST API with API keys/rate limits
- Bulk CSV upload/analyze/download workflow
- Change-alert subscriptions (premium feature candidate)
- Richer provider intelligence (market share over time, migration source/destination, country distribution)
- Detection-quality internal dashboard (avg technologies/domain, avg confidence, avg crawl age) — this one is cheap and arguably should move up if it helps validate Sections 1–3's impact; consider pulling forward as a lightweight metrics view rather than full feature.

**Rationale for deferral:** Every one of these features gets more valuable, not less, if built on top of higher crawl success and richer per-domain evidence. Building the relationship graph or public API against today's data (66% robots-based failure rate on attempted domains, unknown but likely thin per-domain technology counts) means re-deriving/backfilling all of it once Sections 1–3 land anyway.

---

## 6. Rollout sequencing

1. Instrument failure-reason granularity if not already fully in place (confirm current dead-letter schema supports the sub-states in 1.2 and 2).
2. Ship 1.1 → 1.2 → 1.3 (robots.txt), re-pull failure-reason report, confirm recovery.
3. Ship Section 2 (SSRF audit) — independent, can run in parallel with Section 1 if capacity allows.
4. Ship 3.1 (new signal sources) → 3.2 (corpus audit) → 3.3 (confidence scoring).
5. Re-measure: average technologies detected per domain, crawl success rate, false-positive rate on unsafe_target.
6. Scope Section 4 (JS rendering) only after 1–3 are in production and re-measured — its cost/benefit depends on how much of the detection gap remains after 3.1–3.3.
7. Revisit Section 5 roadmap once 1–4 are stable.

---

## 7. Explicit non-goals for this phase

- Do not attempt to defeat TLS fingerprinting (JA3/JA4) or rotate through proxy pools to bypass bot defenses — current data doesn't show this as a meaningful failure driver, and it raises ethical-crawling considerations that deserve separate deliberate discussion, not a bundled fix.
- Do not import third-party rule datasets without explicit license verification.
- Do not weaken SSRF protections without confirmed false-positive evidence per domain (Section 2).
- Do not scale the headless rendering tier beyond the resource ceiling of the current deployment target without a separate infrastructure decision.
