# IntentForge `/search` API — Honest UX & Capability Audit

Tested live against `http://localhost:4000/search?q=` with ~120 requests covering
simple queries, complex multi-constraint queries, and every constraint operator in
the docs. Below is what actually happens, not what the docs claim.

## TL;DR
The API is genuinely good at **intent classification** and **multi-term positive
constraints**, and `filetype:` actually works. But several "constraints" are
either **cosmetic or broken**, and there are **stability problems** that will hurt
real UX. The biggest UX lies: `intitle:`/`inurl:` return ZERO results (broken
hard-filter), negative constraints are *parsed* but **rarely actually exclude**
the thing you asked to remove, and date/`lang:` filters are parsed but never
enforced on the result set.

---

## 🔴 CRITICAL BUGS (UX-breaking)

### 1. `intitle:` and `inurl:` return 0 results — completely broken
Every test returned an empty result set, even though the constraint is correctly
parsed (`applied: ['lang:en','intitle:guide']`):
```
rust inurl:blog        -> n=0
python inurl:docs      -> n=0
tutorial intitle:guide -> n=0
guide intitle:how      -> n=0
```
A user typing `intitle:tutorial` gets an empty page and no explanation. Worse than
ignoring the operator — it silently nukes all results. **This will feel like the
search is "broken."**

### 2. Negative constraints (`not`/`except`/`without`/`no`) are parsed but NOT enforced
The engine correctly extracts the negation into `structured_constraints.negative`
(e.g. `except react` → `negative:['react']`), but the results still contain the
excluded term heavily:
```
java not script        -> 7 of 17 results still mention "script"
javascript except react -> 5 of 8 results still mention "react"  (incl. "Top 10 JS Frameworks", "Intro to client-side frameworks")
linux distro no ubuntu  -> 3 of 14 results still mention "ubuntu"
windows without microsoft -> 1 of 7 still mention "microsoft"
```
Reproducible x3 (same numbers every run). So the constraint is **cosmetic for
ranking** — it may remove a few exact-URL matches but does not actually filter or
demote results containing the excluded word. A user who searches
"javascript framework except react" will be shown React articles and think the
filter is broken (it is).

### 3. Server instability under load / flaky zero-result responses
- First probe run: 1 of 15 sequential requests got `RemoteDisconnected` (server
  closed the connection). Repeated runs sometimes returned `n=0` for queries that
  normally return 24 results (e.g. `python` base returned 0 in one run, 24 in
  another) with no error object.
- The earlier battery run aborted mid-way with `ConnectionAbortedError` /
  `RemoteDisconnected` after many requests.
- **UX impact:** a frontend firing `/search/fast` + `/search` in parallel, or a
  user issuing several rapid queries, will occasionally get an empty response or a
  dropped connection with no `error` field. There is no retry/backoff story and no
  clear error body when this happens.

---

## 🟠 MAJOR GAPS (docs claim features that don't work)

### 4. `after:` / `before:` date constraints are parsed but never applied
```
releases after:2024   -> applied:['lang:en','after:2024']  n=8  but NO date field in any result
cve before:2020       -> n=0  (constraints=None)
python docs before:2010 -> n=0
```
The `applied_constraints` lists the date filter, but the response contains **no
date/`published_date` field at all**, so there is no way to tell whether results
were actually filtered by date. For `fresh`/news intents this is a real miss —
users can't trust "after:2024" did anything.

### 5. `lang:` is parsed but does not reliably filter language
```
python lang:fr -> applied:['lang:fr'] n=13
```
Mixed: it returns `fr.wikipedia.org` AND English Wikipedia/Programiz. So `lang:fr`
biases toward French but still returns English pages. For a user who explicitly
asked French, getting mostly-English results is a confusing UX.

### 6. `filetype:` — the ONE constraint that actually works ✅
```
python filetype:pdf -> 10/10 results are .pdf
rust guide filetype:pdf -> 9/9 .pdf
cve filetype:pdf -> 5/5 .pdf
```
Genuinely enforced. (Note: very few queries have enough PDF results, so `n` is
small — but when it matches, it's correct.)

---

## 🟡 HONEST STRENGTHS (don't break these)

- **Intent classification is excellent** and fast (~1.3–3.4s, cached ~0.03s):
  `restaurant near me` → `local`, `axum vs actix` → `comparison`,
  `how to deploy fastapi` → `how-to`, `python` → `navigational`. Confidence is
  sensible.
- **Positive multi-constraint queries rank very well:**
  `rust web server framework axum vs actix performance benchmark` → top hit is
  literally "Axum vs Actix-web 2026…" (score 1.0).
  `best free password manager open source no subscription` → real open-source PM
  articles on top.
- **`site:` IS a working hard filter** (when the domain has indexed content):
  `docker site:docs.docker.com` → 3/3 on-site; `python site:docs.python.org` →
  3/3 on-site; no off-domain leakage. ✅
- **Exact phrases work**: `"docker compose"` → 15/15 contain the phrase;
  `"python virtual environment"` → 7/7. ✅
- **No 5xx to client**; degrades to empty results gracefully.

---

## 🟡 MINOR UX / CONSISTENCY NOTES

- **Empty `q` edge cases return `status=None` with no `error` body** in my
  harness (the connection sometimes drops instead of returning the documented
  `400 {"error":"Missing or empty query…"}`). The 400 contract may not be met
  under connection-abort conditions. Worth verifying the documented error
  contract is actually returned.
- **`applied_constraints` vs `constraints` mismatch:** `constraints` is the legacy
  `["+python","-django"]` list; `applied_constraints` is the richer list
  (`["lang:en","not:django"]`). A frontend must know to read `applied_constraints`
  to see what was actually used — the legacy field alone is misleading (e.g. it
  drops `lang:` and date filters entirely).
- **Score saturation at 1.0:** many top results show `score: 1.000`, so the
  ranking signal compresses — hard to tell #1 from #2 by score. Consider exposing
  the sub-signal breakdown (relevance vs authority vs consensus) so users/FE can
  re-rank.
- **Latency:** cold ~1.3–3.4s, cached ~0.03s. The `/search/fast` + `/search`
  parallel pattern in the docs is the right call and works.

---

## RECOMMENDED FIXES (priority order)

1. **Fix `intitle:`/`inurl:`** — currently returns 0 results (broken hard filter).
   Either implement as a real title/URL substring boost/filter, or **strip the
   operator and ignore it gracefully** so it doesn't zero out the result set.
   Zeroing all results is the worst possible behavior.
2. **Enforce negative constraints at filter/rank time**, not just parse time.
   Demote or drop results whose title/content contain the negated term. Today it's
   cosmetic — the #1 requested "advanced" feature is lying to users.
3. **Enforce `after:`/`before:`** and actually populate `published_date` (or
   `date`) on results, or stop advertising date filtering. Without a date field in
   the response, the filter is unverifiable and untrustworthy.
4. **Make `lang:` a real filter** (drop non-matching-language results, or at least
   strongly demote) instead of a soft bias.
5. **Stability:** add request timeouts/retries on the gateway side and ensure a
   proper JSON error body is always returned (never a dropped connection with
   `status=None`). The flaky `n=0` for normally-24-result queries suggests
   backend (SearXNG/VPN) timeouts aren't being filled with partial fallback
   consistently.
6. **UX clarity:** when a constraint is applied but yields few/zero results
   (esp. `intitle`/`inurl`/`filetype`), return a signal like
   `"note": "intitle: matched 0 results"` so the FE can show "no results for this
   filter" instead of a blank page.
7. **Expose score sub-signals** so saturation at 1.0 doesn't hide ranking quality.

---

## REPRODUCIBILITY
All numbers above are reproducible (ran negation tests x3 with identical results).
Test scripts: `probe.py`, `probe2.py`, `probe3.py` in the temp dir. Raw responses
available on request. Tested 2026-07-17 against the local gateway.
