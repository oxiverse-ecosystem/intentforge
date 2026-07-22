# Investigation: Two Real Bugs (no fixes applied, per request)

Investigated live against `localhost:4000` + gateway source
(`services/gateway/src/main.rs`) + `docker` gateway logs. Both bugs reproduced.

---

## BUG 1 — `intitle:` / `inurl:` / `intext:` return 0 results (broken hard-filter)

### Symptom (reproduced)
```
rust inurl:blog        -> n=0
python inurl:docs      -> n=0
tutorial intitle:guide -> n=0
guide intitle:how      -> n=0
```
The constraint is parsed correctly (`applied_constraints: ["lang:en","inurl:blog"]`),
but the result set is always empty. Worse than ignoring the operator — it nukes
all results, so the UI looks dead.

### Root cause (two-line chain)
1. **The operator is stripped from the query sent to the search engines.**
   `preprocess_searxng_query()` (main.rs:2778) walks every word and **drops**
   `intitle:`/`inurl:`/`intext:` (and `price:`/`lang:`/`after:`/`before:`) tokens
   without re-emitting them (main.rs:2805-2810):

   ```rust
   if wl.starts_with("intitle:") || wl.starts_with("inurl:") || wl.starts_with("intext:")
       || wl.starts_with("price:") || wl.starts_with("lang:")
       || wl.starts_with("after:") || wl.starts_with("before:")
   { continue; }   // <-- dropped, never re-added
   ```

   Note `site:` IS re-emitted (OR-group, main.rs:2831-2846) — that's why `site:`
   works and `intitle:`/`inurl:` don't. So for `rust inurl:blog` the engines are
   queried with just `rust`.

2. **A hard-filter then drops every result lacking the substring.**
   `filter_result()` (main.rs:1996-2024) hard-drops any result whose
   title/url/content does NOT contain the `intitle`/`inurl` keyword:

   ```rust
   // 4c. Hard filter on inurl:
   if !constraints.inurl.is_empty() {
       let u_low = url.to_lowercase();
       for u in &constraints.inurl {
           if !u_low.contains(&u.to_lowercase()) { return true; }  // drop
       }
   }
   ```

   Because the engines were asked for `rust` only (no `blog`), few/none of the
   returned URLs contain `blog`, so the hard-filter deletes them all → `n=0`.

### Why it's a design flaw, not a typo
The code clearly *intends* these as post-retrieval hard filters, but it never
forwards the keyword to the upstream engine, so there is nothing left to filter.
SearXNG/Bing natively support `intitle:`/`inurl:`, so the operator could simply be
passed through (like `site:` is).

### Fix options (not applied)
- **A (recommended): pass-through.** Stop dropping `intitle:`/`inurl:`/`intext:`
  in `preprocess_searxng_query` (re-emit them like `site:`). Delete the
  hard-filter blocks at 1996-2024 (or downgrade to a *boost*). Minimal, correct.
- **B (softer): make it a relevance boost, not a hard drop.** If the keyword isn't
  in title/url, *demote* instead of *delete*. At least returns results.
- **C (safest for UX): if no results survive the hard-filter, fall back to the
  unfiltered set** and attach `warnings:["intitle: matched 0 results"]` so the FE
  can show "no results for this filter" instead of a blank page.

---

## BUG 2 — Server instability: dropped connections / random `n=0` under concurrency

### Symptom (reproduced)
Firing 8 concurrent requests including 3 identical `python` queries:
```
python n=0 err= HTTPError 408 Request Timeout   <-- FAIL
python n=0 err= TimeoutError: timed out          <-- FAIL
python n=0 err= HTTPError 408 Request Timeout     <-- FAIL
postgres n=0 err= None                            <-- FAIL (empty, no body)
redis    n=0 err= None                            <-- FAIL (empty, no body)
```
Other single/serial requests are fine (python alone → 24 results). So failures
correlate with **concurrency + identical-query fan-out + flaky upstream (gluetun
VPN / SearXNG)**.

### Root cause (two compounding parts)

**(a) Dedup creates a single point of failure for concurrent identical queries.**
When N identical queries arrive, 1 becomes "leader" and the others subscribe via
oneshot channels (main.rs:4913-4937, notify at 7219-7226). All subscribers hang
off the leader's single upstream fetch. Gateway logs confirm the pattern:
```
DEDUP: another request in-flight for 'python', subscribing   (x2)
DEDUP: waiting for in-flight query 'python' to complete
...
DEDUP: notifying 1 waiter(s) for 'python'
```
When the leader's upstream fetch is slow (VPN/SearXNG contention under load), the
gateway's `TimeoutLayer` (main.rs:4627, `Duration::from_secs(20)`) **cancels the
leader task**. Cancelling the leader drops its oneshot senders, so the waiting
subscribers get `RemoteDisconnected` / orphaned — all N copies fail together even
though the query is perfectly valid.

**(b) Upstream (SearXNG via gluetun VPN) is intermittently empty.** Logs show:
```
GARBAGE CLUSTER (best=0.000, mean=0.000) — parallel retry already fired all variations
SEMANTIC FILTER SKIPPED (degenerate scorer, trusting RRF): web_results.len=0
```
When all engines time out, the gateway returns `200 OK` with `results:[]` and **no
error body**. That's "graceful" per the API contract ("never 5xx"), but for UX it's
a silent blank page with `n=0` and no explanation — indistinguishable from a real
"no results" answer.

### Why this matches the earlier report
- `RemoteDisconnected` = leader task cancelled by TimeoutLayer, orphaning
  subscribers (the dedup single-point-of-failure).
- Random `n=0` with no error = upstream returned empty; gateway returns empty
  `results:[]` with no signal.
- A busy FE firing `/search/fast` + `/search` in parallel (the documented pattern)
  will hit exactly this: two concurrent requests, one of which is a duplicate of a
  slow leader → dropped connection.

### Fix options (not applied)
- **For (a):** decouple subscribers from leader failure. On leader timeout/cancel,
  subscribers should **re-execute the query themselves** (or race the leader with a
  per-waiter timeout that falls back to independent execution) instead of relying on
  a single oneshot. Also consider raising/removing the blanket `TimeoutLayer` and
  handling timeouts per-upstream instead.
- **For (b):** when `results` is empty due to upstream failure (vs a genuine
  zero-hit), return a clear signal, e.g. `"error":"upstream_unavailable"` or
  `"warnings":["search backends timed out; showing cached/partial results"]`, so the
  FE can show a retry affordance rather than a blank page. Don't cache empty
  results (already the case at main.rs:7216) and consider a short retry on 408.

---

## Files / lines referenced
- `services/gateway/src/main.rs`
  - `preprocess_searxng_query` strips operators: **2778, 2805-2810** (intitle/inurl
    dropped, never re-emitted — contrast `site:` re-emit at 2831-2846)
  - hard-filter that nukes results: **1996-2024**
  - dedup subscribe/notify: **4913-4937, 7219-7226**
  - global `TimeoutLayer` cancel: **4627** (`Duration::from_secs(20)`)
  - empty-result handling: **7214-7218** (never caches empty — good)

## Verification notes
- intitle/inurl 0-result: reproduced 4/4 queries, every run.
- Instability: reproduced with `max_workers=8` + 3× identical query; serial same
  queries are 100% fine. Gateway logs captured for both failure modes above.
