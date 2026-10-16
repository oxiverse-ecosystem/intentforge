"""ORDER-INVARIANCE PROBE (live, two real gateway processes).

Compares the RANKED URL list of the keys-present gateway (:4000) against the
keys-ABSENT twin (:4001, same image, same netns, same upstreams) for the same
queries. The ONLY difference between the two processes is the affiliate key env.

Because both processes hit the same live upstreams independently, result SETS can
legitimately differ (a web engine can add/drop a URL between two calls). So the
strict contract is checked on the URLs present in BOTH lists: their relative order
must be identical. A pure reordering (the thing affiliate enrichment could cause)
always shows up as an order difference on the intersection, while upstream churn
only changes membership.

Writes a JSON verdict to stdout.
"""
import json
import subprocess
import sys
import urllib.parse
import urllib.request

QUERIES = [
    "iphone 16 pro max price",
    "sony wh-1000xm5 price",
    "best wireless earbuds under 50 dollars",
    "samsung galaxy s24 ultra price",
    "bose quietcomfort ultra headphones",
]
COUNT = 5
KEYS_BASE = "http://localhost:4000"
TWIN = "if-nokeys-twin"


def get(base, path, q):
    url = f"{base}{path}?q={urllib.parse.quote(q)}&count={COUNT}"
    with urllib.request.urlopen(url, timeout=180) as r:
        return json.loads(r.read().decode())


def twin_get(path, q):
    """The twin is only reachable inside gluetun's netns, so curl runs there."""
    url = f"http://127.0.0.1:4001{path}?q={urllib.parse.quote(q)}&count={COUNT}"
    out = subprocess.run(
        [
            "docker", "run", "--rm", "--network", "container:if-dev-gluetun",
            "curlimages/curl:8.11.1", "-sS", "-m", "180", url,
        ],
        capture_output=True, text=True, timeout=300,
    )
    if out.returncode != 0:
        raise RuntimeError(f"twin probe failed: {out.stderr[:300]}")
    return json.loads(out.stdout)


def ranked(body):
    return [r.get("url") for r in body.get("results", [])]


def aff_count(body):
    return sum(1 for r in body.get("results", []) if isinstance(r.get("affiliate"), dict))


verdict: dict = {"queries": [], "pass": True}
for q in QUERIES:
    row = {"query": q}
    for label, path in (("search", "/search"), ("shopping", "/shopping")):
        try:
            a = get(KEYS_BASE, path, q)
            b = twin_get(path, q)
        except Exception as e:  # noqa: BLE001
            row[label] = {"error": f"{type(e).__name__}: {e}"}
            verdict["pass"] = False
            continue
        ru, tu = ranked(a), ranked(b)
        common = [u for u in ru if u in set(tu)]
        rel_tu = [u for u in tu if u in set(common)]
        order_same = common == rel_tu
        row[label] = {
            "keys_ranked": ru,
            "nokeys_ranked": tu,
            "keys_affiliate_blocks": aff_count(a),
            "nokeys_affiliate_blocks": aff_count(b),
            "common_count": len(common),
            "relative_order_identical": order_same,
            "exact_list_identical": ru == tu,
        }
        if not order_same:
            verdict["pass"] = False
    verdict["queries"].append(row)

print(json.dumps(verdict, indent=1))
sys.exit(0 if verdict["pass"] else 1)
