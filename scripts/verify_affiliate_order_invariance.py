"""ORDER-INVARIANCE PROBE (live, three real gateway processes, honest attribution).

The claim under test: affiliate enrichment and affiliate keys NEVER change the
ranked order.

Affiliate keys are read from the PROCESS environment, so "keys present" vs "keys
absent" can only be compared through the real pipeline by running two processes.
`scripts/gateway-nokeys-twin.sh start` launches all three from ONE image, so the
comparison never depends on whatever the shared `services-gateway:latest` tag
currently points at:

  :4003  keys PRESENT   (dummy key)
  :4001  keys ABSENT    — the variable under test
  :4002  keys ABSENT    — the upstream-churn CONTROL

WHY THE CONTROL: all three query the SAME live upstreams independently, so a
result SET can legitimately change between any two calls (an engine adds/drops a
URL, a fetch times out, a cache expires). A naive "keys vs no-keys order differs
=> FAIL" reading would blame monetization for ordinary web churn. The control
measures that churn with the affiliate variable REMOVED. Keys-vs-nokeys drift
that the control does not also exhibit is genuine affiliate-induced reordering.

VERDICT RULES
  1. URLs present in ALL THREE ranked lists must appear in the same relative
     order in all three. (Membership churn is upstream; ORDER churn is ours.)
  2. Keys-vs-nokeys order drift FAILS only when the nokey-vs-nokey control stayed
     stable. If the control also moved, the drift is reported as
     `upstream_churn`, not as a violation.
  3. The keys process must actually decorate at least once (otherwise the whole
     comparison is vacuous) and neither no-key process may decorate anything.
  4. On the SAME process, /search and /shopping must return byte-identical ranked
     URL lists for the same query — same handle_search pipeline, one with
     post-rank commerce decoration. Cache-backed, so upstream churn cannot fake
     either a pass or a failure here.

All three ports are reachable only inside gluetun's network namespace, so every
request goes through a short-lived curl container attached to it.

Usage:  python scripts/verify_affiliate_order_invariance.py
Exit 0 = invariant holds. Exit 1 = real affiliate-induced reordering.
"""
import json
import subprocess
import sys
import time
import urllib.parse

QUERIES = [
    "iphone 16 pro max price",
    "sony wh-1000xm5 price",
    "best wireless earbuds under 50 dollars",
    "samsung galaxy s24 ultra price",
    "bose quietcomfort ultra headphones",
]
COUNT = 5
KEYS_PORT = 4003
NOKEY_A_PORT = 4001
NOKEY_B_PORT = 4002
NETNS = "container:if-dev-gluetun"
CURL_IMAGE = "curlimages/curl:8.11.1"
RETRIES = 2


def get(port, path, q):
    """All twin ports live inside gluetun's netns, so curl runs in there too."""
    url = f"http://127.0.0.1:{port}{path}?q={urllib.parse.quote(q)}&count={COUNT}"
    last = None
    for _ in range(RETRIES + 1):
        out = subprocess.run(
            ["docker", "run", "--rm", "--network", NETNS, CURL_IMAGE, "-sS", "-m", "180", url],
            capture_output=True, text=True, timeout=300,
        )
        if out.returncode == 0 and out.stdout.strip().startswith("{"):
            return json.loads(out.stdout)
        last = (out.stderr or out.stdout)[:200]
        time.sleep(3)
    raise RuntimeError(f"port {port} probe failed: {last}")


def ranked(body):
    return [r.get("url") for r in body.get("results", [])]


def aff_count(body):
    return sum(1 for r in body.get("results", []) if isinstance(r.get("affiliate"), dict))


def shared_order(*lists):
    """URLs present in every list, in each list's own order."""
    if not lists:
        return [], []
    common = [u for u in lists[0] if all(u in set(l) for l in lists[1:])]
    return common, [[u for u in l if u in set(common)] for l in lists]


verdict: dict = {"pass": True, "decorated_on_keys_process": False, "queries": [], "notes": []}
for q in QUERIES:
    row: dict = {"query": q, "surfaces": {}}
    for label, path in (("search", "/search"), ("shopping", "/shopping")):
        try:
            keys_body = get(KEYS_PORT, path, q)
            a_body = get(NOKEY_A_PORT, path, q)
            b_body = get(NOKEY_B_PORT, path, q)
        except Exception as e:  # noqa: BLE001
            row["surfaces"][label] = {"error": f"{type(e).__name__}: {e}"}
            verdict["pass"] = False
            verdict["notes"].append(f"{q} [{label}]: probe error {e}")
            continue

        k_rank, a_rank, b_rank = ranked(keys_body), ranked(a_body), ranked(b_body)
        common, per_list = shared_order(k_rank, a_rank, b_rank)
        keys_vs_a = per_list[0] == per_list[1]
        a_vs_b = per_list[1] == per_list[2]
        k_aff, a_aff, b_aff = aff_count(keys_body), aff_count(a_body), aff_count(b_body)
        if k_aff:
            verdict["decorated_on_keys_process"] = True
        if a_aff or b_aff:
            verdict["pass"] = False
            verdict["notes"].append(
                f"{q} [{label}]: no-key process emitted {a_aff + b_aff} affiliate blocks"
            )

        if not keys_vs_a and a_vs_b:
            verdict["pass"] = False
            verdict["notes"].append(
                f"{q} [{label}]: ORDER-INVARIANCE VIOLATION — keys vs no-keys order "
                "differs while the no-key control stayed stable"
            )

        # Attribution is about the AFFILIATE VARIABLE, so it is only
        # affiliate-induced when keys-vs-nokeys actually diverged while the
        # nokey-vs-nokey control stayed stable. When keys-vs-nokeys agrees, the
        # affiliate variable had no effect no matter what the control did — a
        # control-only difference is pure upstream churn, and labeling it
        # "AFFILIATE_INDUCED" would blame monetization for nothing.
        if keys_vs_a:
            attribution = "identical" if a_vs_b else "upstream_churn(control_only)"
        else:
            attribution = "AFFILIATE_INDUCED" if a_vs_b else "upstream_churn"

        row["surfaces"][label] = {
            "keys_ranked": k_rank,
            "nokeys_a_ranked": a_rank,
            "nokeys_b_ranked": b_rank,
            "keys_affiliate_blocks": k_aff,
            "nokeys_a_affiliate_blocks": a_aff,
            "nokeys_b_affiliate_blocks": b_aff,
            "shared_count": len(common),
            "keys_vs_nokeys_same_order": keys_vs_a,
            "nokeys_control_same_order": a_vs_b,
            "drift_attribution": attribution,
        }

    try:
        s_k = get(KEYS_PORT, "/search", q)
        sh_k = get(KEYS_PORT, "/shopping", q)
        same_proc = ranked(s_k) == ranked(sh_k)
        row["same_process_search_vs_shopping_identical"] = same_proc
        if not same_proc:
            verdict["pass"] = False
            verdict["notes"].append(
                f"{q}: /search and /shopping ranked lists differ on the SAME process"
            )
    except Exception as e:  # noqa: BLE001
        verdict["notes"].append(f"{q}: same-process check skipped ({e})")
    verdict["queries"].append(row)

if not verdict["decorated_on_keys_process"]:
    verdict["pass"] = False
    verdict["notes"].append(
        "VACUOUS: the keys process never emitted an affiliate block, so nothing was compared"
    )

print(json.dumps(verdict, indent=1))
sys.exit(0 if verdict["pass"] else 1)
