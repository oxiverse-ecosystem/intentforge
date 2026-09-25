"""COLD verification for FIX-IF-32 against the LIVE gateway.

Checks the three properties the card asks for:
  1. The three named probes no longer report an uncalibrated raw probe score.
  2. Confidence actually SPREADS across informational queries (not one band).
  3. /intent and /search agree on the label (the FIX-IF-24 invariant).
"""
import json
import statistics
import sys
import urllib.parse
import urllib.request

GATEWAY = "http://localhost:4000"

CARD_PROBES = [
    "why is my internet slow at night even though i pay for high speed",
    "why is tesla named after nikola tesla",
    "why is the apache web server named after a helicopter",
]

# >= 8 informational queries of visibly varying quality, per verification #3.
SPREAD_PROBES = CARD_PROBES + [
    "what is a buffer overflow",
    "how do i install python on windows 11",
    "who wrote the book origin of species",
    "what is the capital of australia",
    "explain recursion to a beginner",
    "why does the moon appear to follow me",
    "what is the difference between ram and ssd",
    "near me coffee shops open now",
]

# FIX-IF-24 regression probes: /intent and /search must agree.
AGREEMENT_PROBES = [
    "buy nike air max 90 running shoes",
    "best laptop under 50000",
    "coffee shops near me open now",
    "today news in india",
    "how to change oil in a car",
    "python rest api framework not flask",
]


def get(path, **params):
    url = f"{GATEWAY}{path}?{urllib.parse.urlencode(params)}"
    with urllib.request.urlopen(url, timeout=60) as r:
        return json.loads(r.read().decode())


def probe(q):
    d = get("/intent", q=q)
    return {
        "q": q,
        "intent": d.get("intent"),
        "confidence": d.get("confidence"),
        "calibrated": d.get("confidence_calibrated"),
        "probe": d.get("probe_probability"),
    }


print("=" * 78)
print("1. THE THREE CARD PROBES (live)")
print("=" * 78)
card = [probe(q) for q in CARD_PROBES]
for p in card:
    print(f"  conf={p['confidence']!s:<22} calibrated={p['calibrated']!s:<6} "
          f"probe={p['probe']!s:<22} {p['intent']:<14} {p['q'][:44]}")

confs = [p["confidence"] for p in card]
print(f"\n  ordering (was anti-correlated): {[round(c,4) for c in confs]}")
print(f"  all calibrated: {all(p['calibrated'] for p in card)}")
print(f"  all expose a probe probability: {all(p['probe'] is not None for p in card)}")

print()
print("=" * 78)
print("2. SPREAD ACROSS %d PROBES (verification #3)" % len(SPREAD_PROBES))
print("=" * 78)
spread = [probe(q) for q in SPREAD_PROBES]
for p in sorted(spread, key=lambda x: -(x["confidence"] or 0)):
    print(f"  conf={round(p['confidence'],4):<8} calibrated={p['calibrated']!s:<6} "
          f"{p['intent']:<14} {p['q'][:50]}")

vals = [p["confidence"] for p in spread if p["confidence"] is not None]
print(f"\n  n={len(vals)}  min={min(vals):.4f}  max={max(vals):.4f}")
print(f"  range={max(vals)-min(vals):.4f}  stdev={statistics.pstdev(vals):.4f}")
print(f"  OLD informational band was 0.254-0.474 (range 0.22)")
print(f"  NEW range is {'WIDER' if (max(vals)-min(vals)) > 0.22 else 'NOT wider'} "
      f"than the old compressed band")
uniq = len({round(v, 3) for v in vals})
print(f"  distinct values (3dp): {uniq}/{len(vals)}")

print()
print("=" * 78)
print("3. /intent <-> /search AGREEMENT (verification #4, FIX-IF-24)")
print("=" * 78)
agree = 0
for q in AGREEMENT_PROBES:
    i = get("/intent", q=q).get("intent")
    s = get("/search", q=q).get("intent")
    ok = (i == s)
    agree += ok
    print(f"  {'OK  ' if ok else 'DIFF'}  /intent={str(i):<14} /search={str(s):<14} {q[:44]}")
print(f"\n  agreement: {agree}/{len(AGREEMENT_PROBES)}")

print()
print("=" * 78)
print("4. RAW JSON (first probe) -- full field set")
print("=" * 78)
print(json.dumps(get("/intent", q=CARD_PROBES[0]), indent=2)[:1200])

ok = (agree == len(AGREEMENT_PROBES)
      and (max(vals) - min(vals)) > 0.22
      and all(p["calibrated"] for p in card))
print()
print("=" * 78)
print("VERDICT:", "PASS" if ok else "INCOMPLETE")
print("=" * 78)
sys.exit(0 if ok else 1)
