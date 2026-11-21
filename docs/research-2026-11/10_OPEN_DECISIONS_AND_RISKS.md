# 10 — Open Decisions and Risks

**Status:** For founder review and approval.
**Companion documents:** all others in this package.

This document records every decision that was **not** self-evident, the options considered, the evidence, and the recommendation. Items marked **FOUNDERS DECISION REQUIRED** need explicit approval before implementation.

---

## 1. Decision Register

| # | Decision | Options | Recommendation | Status |
|---|---|---|---|---|
| D1 | Initial crawl scope | Open web vs. curated domains | **Curated** | FOUNDER APPROVAL |
| D2 | Headless rendering in MVP | Include vs. defer | **Defer** | FOUNDER APPROVAL |
| D3 | Search index engine | PostgreSQL FTS vs. Tantivy | **PostgreSQL FTS first** | FOUNDER APPROVAL |
| D4 | Retention periods | Short vs. long per content type | **Short default, per-type overrides** | FOUNDER APPROVAL |
| D5 | User-submitted URLs | Include vs. defer | **Defer** | FOUNDER APPROVAL |
| D6 | Price display from crawled pages | Permit vs. prohibit | **Prohibit until per-source review** | LEGAL REQUIRED |
| D7 | Commerce source priority | India-first vs. global-first | **Global-first (fewer legal barriers)** | FOUNDER APPROVAL |
| D8 | Affiliate revenue model | Affiliate vs. subscription vs. merchant tools | **Affiliate first, transparent** | FOUNDER APPROVAL |
| D9 | Identity auto-merge thresholds | Conservative vs. aggressive | **Conservative** | Approved by design |
| D10 | Query log retention | 30 / 90 / 365 days | **90 days** | FOUNDER APPROVAL |
| D11 | Takedown response SLA | 24h / 7d / 30d | **24h for suppression** | FOUNDER APPROVAL |
| D12 | Crawl egress | VPN only vs. VPN + Tor | **VPN + Tor with per-domain routing** | Approved by design |
| D4A | Price history retention | Uniform vs. per-source | **Per-source ceilings; none for Amazon** | FOUNDER APPROVAL |

---

## 2. Detailed Decisions

### D1 — Initial Crawl Scope

**Options:**
- **Open web:** discover and crawl anything reachable.
- **Curated domains:** an explicit, reviewed list of permitted domains.

**Evidence:** A small independent engine cannot compete on coverage. Every credible independent engine (Marginalia, DuckDuckGo's curated instant answers, Brave's own index) either curates or partners for coverage. Open-web crawling also maximises legal and abuse exposure.

**Consequences of curated:**
- Smaller index, but defensible and high-quality.
- Requires an editorial process for domain admission.
- Makes the benchmark suite meaningful (known ground truth).

**Consequences of open web:**
- Much larger exposure to crawl traps, blocks, and legal claims.
- Requires substantially more infrastructure.
- Makes quality measurement difficult.

**Recommendation: curated.** Start with 50-200 domains.

---

### D2 — Headless Rendering in the MVP

**Options:** include from the start, or defer.

**Evidence:**
- Rendering costs ~350 MB - 1 GB RAM per concurrent browser.
- It adds 1-5 seconds per page.
- It increases legal exposure on sites whose terms prohibit automated access.
- Most content needed for the MVP is available in static HTML.

**Recommendation: defer.** Add rendering only when a measured need appears for a specific, permitted domain.

---

### D3 — Search Index Engine

**Options:** PostgreSQL FTS + pg_trgm, or Tantivy.

**Evidence:**
- The locked decision names PostgreSQL as the primary database and SQLx for access.
- The existing indexer uses Tantivy, which is an embedded library, not a client-server store.
- Running both means two consistency obligations.

**Consequences of PostgreSQL FTS:**
- One system of record; transactional integrity; SQL-level audit.
- Ranking flexibility is lower than a dedicated engine.
- Must be measured before committing.

**Consequences of Tantivy:**
- Better ranking primitives and proven at scale.
- Second datastore; rebuild and consistency logic.

**Recommendation: PostgreSQL FTS as primary.** Measure against the benchmark; add Tantivy only if a specific, measured gap appears. **This decision must be validated in Phase 0 with a real query set.**

---

### D4 — Retention Periods

**Options:** short (days), medium (months), long (years).

**Evidence:** Retention is a legal and trust question as much as a technical one. Shorter retention reduces risk but limits price-history value.

**Recommendation:**
| Data | Default retention |
|---|---|
| Raw page bodies | 24 hours |
| Extracted page metadata | 12 months |
| Price observations | 24 months |
| Availability observations | 6 months |
| Audit records | 7 years |
| Query logs | 90 days |

**Founder approval required** for each period.

---

### D5 — User-Submitted URLs

**Options:** accept from the public, or defer.

**Evidence:** Submissions are an abuse vector (spam, SEO manipulation, legal pressure) and require identity verification and review tooling.

**Recommendation: defer** until the curated index and review workflow are proven.

---

### D6 — Price Display from Crawled Pages

**Options:** display extracted prices, or prohibit.

**Evidence:** This is the highest-legal-risk decision in the project. Public availability does not imply permission to extract and redistribute. Database rights (EU), contract terms, and price-modelling restrictions (e.g. eBay's API terms) all bear on it.

**Recommendation: prohibit by default.** Prices may only be displayed from sources with an explicit `may_redistribute = true` permission, obtained through an official API, licensed feed, or partner agreement.

**This decision requires legal review, not just founder approval.**

---

### D7 — Commerce Source Priority

**Options:** India-first or global-first.

**Evidence:**
- Indian affiliate programmes (Flipkart, Amazon India) may require residency, GST registration, or local entity status.
- Global programmes (Best Buy API, Awin, CJ) have more accessible documentation and free tiers.
- The locked decision requires both India and global coverage.

**Recommendation: global-first for the MVP**, adding Indian sources as access is verified. This avoids blocking the MVP on residency questions.

---

### D8 — Revenue Model

**Options:** affiliate commissions, subscriptions, merchant tools.

**Evidence:** The product principles forbid advertising and paid ranking. Affiliate commissions are compatible **only if** ranking is structurally independent of commission.

**Recommendation:** affiliate first, with the structural guarantee (no commission field in the ranking path) enforced by a test. Merchant tools and subscriptions as later options.

**Founder approval required.**

---

### D9 — Identity Auto-Merge Thresholds

**Recommendation: conservative.** Never auto-merge without a strong identifier. Thresholds are configuration and must be tuned against the benchmark.

**Approved by design** — no founder decision needed unless the thresholds are changed.

---

### D10 — Query Log Retention

**Recommendation: 90 days**, salted hashes by default, no raw text unless a specific need is approved.

**Founder approval required.**

---

### D11 — Takedown Response SLA

**Options:** 24 hours, 7 days, 30 days.

**Recommendation: 24 hours for suppression**, with a longer window for review and appeal. Suppression is cheap; review can take longer.

**Founder approval required.**

---

### D12 — Crawl Egress

**Recommendation: VPN + Tor with per-domain routing.** Domains that block datacenter IPs can be routed through Tor; others use the VPN for speed.

**Approved by design.**

---

## 3. Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | Commerce source access denied | High | MVP blocked | Synthetic connector fallback; global-first sourcing |
| R2 | Legal claim over extraction/redistribution | Medium | Severe | Permissions model; legal review; prohibit by default (D6) |
| R3 | Crawler blocked / IP banned | High | Medium | Strict budgets; honest User-Agent; Tor fallback |
| R4 | PostgreSQL FTS insufficient for ranking | Medium | Medium | Measure in Phase 0; Tantivy fallback |
| R5 | Identity over-merging destroys trust | Medium | Severe | Conservative thresholds; review queue |
| R6 | Scope creep into open-web crawling | High | High | Curated scope enforced by policy |
| R7 | Rendering cost overruns | Medium | Medium | Deferred (D2) |
| R8 | Founder review bottleneck | Medium | Medium | Decision register; async approval |
| R9 | Dependency vulnerability in a parser | Medium | Medium | `cargo audit`; memory-safe crates; size caps |
| R10 | Ranking contaminated by commercial pressure | Low | Severe | Structural exclusion; test enforcement |
| R11 | Team builds infrastructure before value | High | High | MVP boundary; phase gates |
| R12 | Retention periods set too long | Medium | Medium | Short defaults; per-type justification |

---

## 4. Decisions Requiring Founder Approval

The following cannot proceed without explicit approval:

1. **D1** — Curated crawl scope and the initial domain admission process.
2. **D2** — Deferring headless rendering.
3. **D3** — PostgreSQL FTS as the primary search engine.
4. **D4** — Retention periods per content type.
5. **D5** — Deferring user-submitted URLs.
6. **D6** — Prohibition on displaying crawled prices without explicit permission *(also requires legal review)*.
7. **D7** — Global-first commerce sourcing.
8. **D8** — Affiliate-first revenue model with structural ranking independence.
9. **D10** — 90-day query log retention.
10. **D11** — 24-hour takedown suppression SLA.
11. **D4A** — Per-source price-history retention, and specifically that Amazon-sourced price history must not be retained.

---

## 4A. Phase Acceptance Gates (Summary)

Full criteria are in `08_BENCHMARK_PLAN.md §9`. Summary:

| Gate | Phase | Criteria |
|---|---|---|
| G1 | Phase 2 complete | crawl success ≥ 0.90; politeness 1.00; zero SSRF bypasses |
| G2 | Phase 3 complete | Precision@10 ≥ 0.65; search p95 ≤ 800 ms |
| G3 | Phase 5 complete | identity precision ≥ 0.98; no weak auto-merges |
| G4 | Phase 6 complete | product Precision@5 ≥ 0.75; offer correctness ≥ 0.99 |
| G5 | Phase 7 complete | all gates green under load and failure injection |

**Phase 7 (Benchmarking, Hardening, Controlled Expansion)** is the ongoing phase. Its acceptance criterion is that every earlier gate remains green under load testing and failure injection, and that the security audit and legal review of all enabled sources are complete.

Any decision in this register that would change a gate threshold must be re-approved by the founder.

---

## 5. Recommended First Milestone

**Milestone M1 (end of Phase 1 + first half of Phase 2):**

> A running system that crawls **20 curated domains** politely with full policy enforcement, stores everything in PostgreSQL with provenance, and serves a public web search API whose results are explainable.

**Why this milestone:**
- It proves the entire critical path without commerce complexity.
- It produces something a user can try.
- It generates the real-world data needed to calibrate the benchmarks.
- It avoids the two classic traps: scale before value, and product database before search.

**Success criteria:** Gate G1 met; 20 domains crawled; politeness 100%; zero SSRF bypasses; search p95 ≤ 800 ms on the benchmark corpus.

---

## 6. Phase 1 — Anonymous Goals Decision Record (accepted 2026-11-21)

Accepted from `docs/IMPLEMENTATION_PLAN_GOALS_SUPABASE.md` §0. This record is additive to D1–D12 above, which govern the WebGraph/commerce research package. The identifiers below are the plan's own D1–D11 and must not be read as amendments to the register in §1.

| # | Decision (Phase 1 anonymous goals) | Status |
|---|---|---|
| D1 | Anonymous goals are the only supported mode. A browser-generated `owner_ref` (UUID v4, localStorage `if_owner_ref`) is the sole identity, sent as `X-Anonymous-Owner` header only — never in a query string. | Accepted |
| D2 | Goals and their `owner_ref` expire on a sliding 6-hour TTL of last goal activity (`IF_ANON_GOAL_TTL_HOURS=6`); on expiry rows are deleted, not anonymised. | Accepted |
| D3 | No login in Phase 1: no Google OAuth, no JWT verification, no `POST /goals/claim`; all auth scaffolding stays dormant. | deferred — Phase 2 |
| D4 | `GET /goals/leaderboard` keeps its 200/bare-array contract but returns `[]`; the view and consents table are created dormant for Phase 2, with a UI empty-state placeholder. | deferred — Phase 2 |
| D5 | Hiring notices are a separate, default-off, revocable consent — schema only in Phase 1, with no UI, no delivery and no company access. | deferred — Phase 2 |
| D6 | Supabase is hosted, not self-hosted in Compose; no new containers, only env vars on the gateway. | Accepted |
| D7 | The gateway talks to Supabase over HTTPS per request (PostgREST + RPC); no `sqlx`/PgPool because of gluetun VPN IP rotation; `reqwest` is used. | Accepted |
| D8 | The in-memory store stays the default (`IF_GOAL_STORE=in_memory`); all 27 existing tests pass without Supabase. | Accepted |
| D9 | Wire contract unchanged (`goal_<zero-padded digits>`, 1-indexed phases, bare arrays for `/goals` and `/goals/leaderboard` capped at 50, score = `completed_phases*100` + 500 all-complete bonus); only additive field is `expires_at`. | Accepted |
| D10 | No JWT is expected on any goal request in Phase 1; a `Bearer` value is treated as the legacy service API key. | Accepted |
| D11 | Use the new publishable/secret API keys (`sb_publishable_*` / `sb_secret_*`); legacy `anon`/`service_role` names are fallback only. Non-JWT keys go on the `apikey` header only, never `Authorization: Bearer`. | Accepted |

D3, D4 and D5 are recorded as **deferred — Phase 2**: no OAuth, claim endpoint, account attachment, consent UI, leaderboard entries or hiring-notice delivery may be implemented in Phase 1. See `docs/IMPLEMENTATION_PLAN_GOALS_SUPABASE.md` §9.

---

*This document is the authoritative list of unresolved decisions. Nothing here is approved until the founder signs off.*
