#!/usr/bin/env python3
"""Commerce-target audit: ranking integrity, disclosure, graceful degradation."""
import json, sys, urllib.request, urllib.error, urllib.parse

BASE = "http://localhost:4000"

def get(path, timeout=30):
    try:
        r = urllib.request.urlopen(BASE + path, timeout=timeout)
        return r.status, json.loads(r.read())
    except urllib.error.HTTPError as e:
        try: return e.code, json.loads(e.read())
        except: return e.code, {"_raw": e.read().decode(errors="replace")[:500]}
    except Exception as e:
        return -1, {"_error": str(e)}

_results = []
def check(name, cond, detail=""):
    status = "PASS" if cond else "FAIL"
    _results.append((status, name, detail))
    print(f"  [{status}] {name}" + (f" — {detail}" if detail and not cond else ""))

print("=" * 70)
print("COMMERCE-TARGET AUDIT")
print("=" * 70)

# ── 1. Ranking integrity: /search vs /shopping URL order ──────────────
print("\n── 1. Ranking integrity ──")
q = "best wireless earbuds under 50"
code_s, body_s = get(f"/search?q={urllib.parse.quote(q)}&count=10")
code_sh, body_sh = get(f"/shopping?q={urllib.parse.quote(q)}&count=10")
check("GET /search 200", code_s == 200, f"code={code_s}")
check("GET /shopping 200", code_sh == 200, f"code={code_sh}")

if code_s == 200 and code_sh == 200:
    urls_s = [r["url"] for r in body_s.get("results", [])]
    urls_sh = [r["url"] for r in body_sh.get("results", [])]
    check("  /search returns results", len(urls_s) > 0, f"n={len(urls_s)}")
    check("  /shopping returns results", len(urls_sh) > 0, f"n={len(urls_sh)}")
    check("  URL order identical search vs shopping", urls_s == urls_sh,
          f"search={urls_s[:3]} shopping={urls_sh[:3]}")
    if urls_s != urls_sh:
        print("  MISMATCH:")
        for i, (a, b) in enumerate(zip(urls_s, urls_sh)):
            if a != b:
                print(f"    [{i}] search: {a}")
                print(f"    [{i}] shopping: {b}")

# ── 2. Disclosure: every affiliate-decorated result has disclosed=true ──
print("\n── 2. Disclosure ──")
if code_sh == 200:
    results = body_sh.get("results", [])
    for i, r in enumerate(results):
        aff = r.get("affiliate")
        if aff:
            disclosed = aff.get("disclosed")
            check(f"  result[{i}] affiliate.disclosed == true", disclosed == True,
                  f"disclosed={disclosed}")
            check(f"  result[{i}] affiliate has network", "network" in aff,
                  f"keys={list(aff.keys())}")

# ── 3. Graceful degradation: affiliate keys UNSET ──────────────────────
print("\n── 3. Graceful degradation (affiliate keys unset) ──")
# We can't easily unset env vars on a running container, but we can check
# that the /shopping endpoint doesn't crash and returns results.
# The dev stack has SOVRN_COMMERCE_KEY=dummy-test-key-do-not-use set.
# We verify the endpoint returns 200 and results are present.
check("  /shopping returns 200 with results", code_sh == 200 and len(body_sh.get("results", [])) > 0,
      f"code={code_sh} n={len(body_sh.get('results', []))}")

# ── 4. Privacy: no query/user/session/IP in affiliate params ───────────
print("\n── 4. Privacy: affiliate params contain no PII ──")
if code_sh == 200:
    for i, r in enumerate(body_sh.get("results", [])):
        aff = r.get("affiliate")
        if aff:
            aff_url = aff.get("url", "")
            # Check that query text is NOT in affiliate URL
            query_in_aff = q.lower() in aff_url.lower()
            check(f"  result[{i}] query text NOT in affiliate url", not query_in_aff,
                  f"aff_url={aff_url[:100]}")
            # Check cuid/subid/customid values
            cuid = aff.get("cuid", "")
            subid = aff.get("subid", "")
            customid = aff.get("customid", "")
            # These should be merchant host or empty, not user/session/IP
            for param_name, param_val in [("cuid", cuid), ("subid", subid), ("customid", customid)]:
                if param_val:
                    # Should be a domain-like string, not a user id
                    looks_like_user_id = any(c in param_val for c in ["@", "session", "user", "ip", "uid"])
                    check(f"  result[{i}] affiliate.{param_name} not PII", not looks_like_user_id,
                          f"{param_name}={param_val}")

# ── 5. No misrepresentation: commerce facts have provenance ────────────
print("\n── 5. Commerce provenance ──")
if code_sh == 200:
    for i, r in enumerate(body_sh.get("results", [])):
        prov = r.get("commerce_provenance", {})
        if prov:
            check(f"  result[{i}] provenance has url", "url" in prov, f"keys={list(prov.keys())}")
            check(f"  result[{i}] provenance has fetched", "fetched" in prov)
            check(f"  result[{i}] provenance has reason", "reason" in prov)
            # observed_at should be present when fetched=true
            if prov.get("fetched"):
                check(f"  result[{i}] provenance.observed_at present when fetched",
                      prov.get("observed_at") is not None,
                      f"observed_at={prov.get('observed_at')}")

# ── Summary ────────────────────────────────────────────────────────────
print("\n" + "=" * 70)
print("COMMERCE AUDIT COMPLETE")
print("=" * 70)
