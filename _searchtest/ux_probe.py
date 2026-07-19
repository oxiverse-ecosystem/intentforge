#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Honest UX probe for IntentForge /search at localhost:4000.

Records raw responses AND computes UX-failure metrics:
  - constraint leakage (negated term still present in results)
  - positive-constraint coverage
  - score saturation / collapse (ranking meaningless)
  - intent sanity (junk/chitchat misclassified)
  - spell corruption (real word silently swapped)
  - query_quality flag / graceful degradation
  - freshness handling for temporal queries
  - latency per query
Outputs _searchtest/ux_probe_results.json
"""
import json, urllib.parse, urllib.request, sys, time, re, os

BASE = "http://localhost:4000/search"
OUT = os.path.join(os.path.dirname(__file__), "ux_probe_results.json")

NON_LATIN = re.compile(r"[\u0400-\u04ff\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af\u0600-\u06ff]")

def ask(q, timeout=35):
    url = f"{BASE}?q=" + urllib.parse.quote(q)
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "uxprobe/1.0"})
        with urllib.request.urlopen(req, timeout=timeout) as r:
            data = json.loads(r.read().decode())
            return data, r.status, None, time.time() - t0
    except urllib.error.HTTPError as e:
        body = e.read().decode(errors="replace")
        try:
            data = json.loads(body)
        except Exception:
            data = {"raw": body}
        return data, e.code, None, time.time() - t0
    except Exception as e:
        return None, None, str(e), time.time() - t0

def norm(s):
    return re.sub(r"[^a-z0-9]", "", (s or "").lower())

def measures(d, q):
    """Compute UX-failure metrics for one response."""
    m = {}
    res = d.get("results", []) or []
    m["n_results"] = len(res)

    # ---- constraint leakage ----
    cons = d.get("constraints", []) or []
    negatives = [c[1:] for c in cons if c.startswith("-")]
    positives = [c[1:] for c in cons if c.startswith("+")]
    # also try structured_constraints
    sc = d.get("structured_constraints") or {}
    negatives += sc.get("negative", []) or []
    positives += sc.get("positive", []) or []
    negatives = [x for x in dict.fromkeys(negatives) if x]
    positives = [x for x in dict.fromkeys(positives) if x]

    leaked = []
    for neg in negatives:
        nh = norm(neg)
        if not nh:
            continue
        hits = 0
        for r in res:
            blob = norm((r.get("title", "") or "") + " " + (r.get("content", "") or ""))
            if nh in blob:
                hits += 1
        if hits:
            leaked.append({"term": neg, "results_with_term": hits, "pct": round(hits / len(res), 3) if res else 0})
    m["negative_constraints"] = negatives
    m["positive_constraints"] = positives
    m["constraint_leaks"] = leaked
    m["constraint_leak_any"] = bool(leaked)

    # positive coverage
    pos_cov = []
    for p in positives:
        ph = norm(p)
        if not ph:
            continue
        hits = sum(
            1 for r in res
            if ph in norm((r.get("title", "") or "") + " " + (r.get("content", "") or ""))
        )
        pos_cov.append({"term": p, "coverage": round(hits / len(res), 3) if res else 0})
    m["positive_coverage"] = pos_cov

    # ---- score saturation / collapse ----
    scores = [float(r.get("score", 0) or 0) for r in res]
    if scores:
        sat = sum(1 for s in scores if s >= 0.999)
        m["score_min"] = round(min(scores), 4)
        m["score_max"] = round(max(scores), 4)
        m["score_unique"] = len(set(round(s, 4) for s in scores))
        m["score_saturated_frac"] = round(sat / len(scores), 3)
        m["score_all_equal"] = (m["score_unique"] == 1)
    else:
        m["score_min"] = m["score_max"] = m["score_unique"] = 0
        m["score_saturated_frac"] = 0.0
        m["score_all_equal"] = True

    # ---- non-latin leak (english queries pulling foreign results) ----
    qlatin = norm(q) and not NON_LATIN.search(q)
    nonlatin = 0
    for r in res:
        if NON_LATIN.search((r.get("title", "") or "") + " " + (r.get("content", "") or "")[:200]):
            nonlatin += 1
    m["nonlatin_result_frac"] = round(nonlatin / len(res), 3) if res else 0
    m["query_is_latin"] = qlatin

    # ---- misc flags ----
    m["intent"] = d.get("intent")
    m["category"] = d.get("category")
    m["confidence"] = d.get("confidence")
    m["query_quality"] = d.get("query_quality")
    m["spell_corrected"] = d.get("spell_corrected")
    m["has_message"] = bool(d.get("message"))
    m["warnings"] = d.get("warnings") or []
    m["ignored"] = d.get("ignored") or []
    return m

# ---- query battery: focused on COMPLEX + CONSTRAINTS ----
battery = [
    # --- constraint / negation (core UX) ---
    ("monitoring tools not prometheus", "negation"),
    ("python web framework for beginners not django", "negation+constraint"),
    ("browser not chrome not edge not firefox", "multi-negation"),
    ("text editor without vim", "without-negation"),
    ("javascript framework except react", "except-negation"),
    ("linux distro no ubuntu", "no-negation"),
    ("search engine alternative to google", "alternative-to"),
    ("programming language other than java", "other-than"),
    ("static site generator instead of jekyll", "instead-of"),
    ("database excluding mongodb", "excluding-negation"),
    ("best laptop for video editing not macbook", "negation+spec"),
    ("how to learn rust programming without prior experience", "without-phrase"),
    ("vegan restaurants in seattle", "spell-corruption-risk"),
    ("python tutorial", "no-junk-official"),
    # --- operator-style constraints ---
    ("python tutorial site:docs.python.org filetype:pdf", "site+filetype"),
    ("kubernetes docs after:2023", "date-constraint"),
    ("rust programming intitle:async", "intitle"),
    ("docker compose inurl:reference", "inurl"),
    # --- complex comparisons ---
    ("rust vs go for backend development", "comparison"),
    ("postgres vs mysql vs sqlite for a small web app", "triple-comparison"),
    ("react vs vue vs angular for large enterprise app", "triple-comparison"),
    ("nextjs vs remix vs astro which is best in 2026", "comparison+fresh"),
    # --- temporal / fresh ---
    ("latest ai news 2026", "fresh"),
    ("recent rust releases", "fresh"),
    ("breaking news today ukraine", "fresh+breaking"),
    ("what happened in tech this week", "fresh-week"),
    # --- local ---
    ("coffee shops near me", "local"),
    ("best pizza near me open now", "local+fresh"),
    ("indian restaurants in new delhi", "local-geo"),
    # --- how-to / technical ---
    ("how to deploy docker container to kubernetes", "how-to"),
    ("how do i reset my router", "how-to"),
    ("kubernetes ingress tls configuration", "technical"),
    ("python asyncio event loop explained", "technical"),
    ("configure nginx reverse proxy with ssl", "technical"),
    # --- transactional ---
    ("buy noise cancelling headphones under 200", "transactional+price"),
    ("cheap flights to tokyo", "transactional"),
    ("best gaming laptop under 1500 with rtx 4070", "transactional+spec"),
    # --- factual ---
    ("population of France 2025", "factual"),
    ("what is the speed of light", "factual"),
    ("distance from earth to moon in km", "factual"),
    # --- chitchat / impossible / gibberish ---
    ("how are you", "chitchat"),
    ("tell me a joke", "chitchat"),
    ("what's the meaning of life", "chitchat"),
    ("best way to fly to mars for free", "impossible"),
    ("how to become immortal in 3 days", "impossible"),
    ("asdfghjkl qwerty zxcvbnm", "gibberish"),
    ("wergwreg weubf iweb", "gibberish"),
    # --- edge cases ---
    ("", "empty"),
    ("a", "single-char"),
    ("the", "stopword-only"),
    ("12345", "no-alpha"),
]

if __name__ == "__main__":
    results = []
    start = time.time()
    for q, label in battery:
        d, status, err, lat = ask(q)
        rec = {"q": q, "label": label, "status": status, "latency_s": round(lat, 2), "error": err}
        if d is not None:
            rec["response"] = d
            rec["metrics"] = measures(d, q)
        results.append(rec)
        # live progress
        n = len(results)
        if d is None:
            print(f"[{n}/{len(battery)}] ERR q={q!r} {err}")
        else:
            mt = rec["metrics"]
            flag = ""
            if mt.get("constraint_leak_any"):
                flag += " LEAK"
            if mt.get("score_all_equal") and mt.get("n_results", 0) > 1:
                flag += " FLAT-SCORE"
            if mt.get("nonlatin_result_frac", 0) > 0.5 and mt.get("query_is_latin"):
                flag += " L10N-LEAK"
            intent = mt.get("intent")
            conf = mt.get("confidence")
            print(f"[{n}/{len(battery)}] {label:<22} n={mt.get('n_results'):>2} "
                  f"intent={str(intent):<13} conf={conf if conf is None else round(conf, 2)} "
                  f"lat={lat:5.1f}s{flag}")
    out = {"generated": time.strftime("%Y-%m-%dT%H:%M:%S"), "queries": len(battery),
           "total_time_s": round(time.time() - start, 1), "results": results}
    with open(OUT, "w") as f:
        json.dump(out, f, indent=2)
    print(f"\nWrote {OUT}")
    print(f"Total: {out['total_time_s']}s for {len(battery)} queries")
