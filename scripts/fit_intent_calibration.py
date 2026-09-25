#!/usr/bin/env python3
"""Fit the IntentForge intent-confidence calibration artifact.

WHY THIS EXISTS (FIX-IF-32)
---------------------------
The intent-engine used to publish `confidence = 0.25 + (top1 - top2)` -- a
synthetic margin score, not a probability -- and then ~20 lexical overrides in
the engine and gateway clamped it with hardcoded `.max(0.6 / 0.7 / 0.75 / 0.85 /
0.88 / 0.9)`. Measured on the 374-row labeled corpus:

    reported label != distribution argmax ....... 341/374 (91%)
    reported confidence > p(reported label) ...... 351/374 (94%), mean +0.306
    AUC(reported confidence) for right vs wrong . 0.555   <- chance
    AUC(p(reported label)) for right vs wrong ... 0.788
    literal 0.600 emitted on ..................... 88 rows
    informational band (p05-p95) ................. 0.254 - 0.474

So the number did not describe the label the API actually returned.

WHAT THIS DOES
--------------
Fits Platt scaling that maps the engine's own `p(reported label)` onto the
empirically measured probability that the reported label is correct. Platt
scaling is a monotone transform, so it keeps the 0.788 discriminative power
while turning the number into an actual probability.

The coefficients are DATA, loaded at runtime by the intent-engine, and this
script is the only way they are produced. Refit on a larger corpus to improve
from experience. There is no per-query table and no query -> confidence map.

USAGE
-----
    # 1. collect live engine output for the labeled corpus
    #    (each row of the corpus is replayed through /analyze)
    python scripts/fit_intent_calibration.py --collect \
        --corpus services/intent-engine/config/calibration_corpus.csv \
        --observed /tmp/observed.json
    # 2. fit and write the artifact
    python scripts/fit_intent_calibration.py --fit \
        --observed /tmp/observed.json \
        --out services/intent-engine/config/intent_calibration.json
"""
import argparse
import csv
import json
import math
import os
import subprocess
import sys
import urllib.parse

EPS = 1e-6

# Where the intent-engine listens from inside the compose network. The gateway
# shares the gluetun netns with the engine, so we exec inside the gateway
# container rather than publishing the engine's port on the host.
CONTAINER = os.environ.get("IF_GATEWAY_CONTAINER", "if-dev-gateway")
ENGINE_URL = os.environ.get("IF_ENGINE_URL", "http://127.0.0.1:3005/analyze")


def logit(p):
    p = min(max(p, EPS), 1.0 - EPS)
    return math.log(p / (1.0 - p))


def collect(corpus, observed_path, container):
    """Replay every labeled query through the live engine and record what it said."""
    rows = list(csv.DictReader(open(corpus, encoding="utf-8")))
    lines = []
    for r in rows:
        q = r["query"]
        sq = q.replace("'", "'\\''")
        lines.append(
            "printf '%s\\t' '" + sq + "'; "
            "curl -s -m 60 --get --data-urlencode 'q=" + q.replace("'", "") + "' "
            + ENGINE_URL + "; echo"
        )
    script = "\n".join(lines).replace("\r", "") + "\n"

    # Feed the script over stdin as BYTES. Two Windows traps this avoids:
    #   * passing it as argv blows the 32k command-line limit on a 374-row corpus
    #   * text=True rewrites \n -> \r\n, so every `echo` arrives as "echo\r"
    res = subprocess.run(
        ["docker", "exec", "-i", container, "sh", "-s"],
        input=script.encode("utf-8"),
        capture_output=True,
    )
    text = res.stdout.decode("utf-8", "replace")
    if not text.strip():
        sys.exit("collection produced no output; stderr=" + res.stderr.decode("utf-8", "replace")[:500])

    out = []
    for line, r in zip(text.splitlines(), rows):
        _, _, payload = line.partition("\t")
        try:
            d = json.loads(payload)
        except ValueError:
            continue
        out.append({
            "query": r["query"],
            "expected": r["expected"],
            "reported": d.get("intent"),
            "raw_confidence": d.get("confidence"),
            "distribution": d.get("distribution") or {},
        })
    json.dump(out, open(observed_path, "w", encoding="utf-8"), indent=1)
    print("collected %d/%d -> %s" % (len(out), len(rows), observed_path))


def fit(observed_path, out_path):
    recs = [r for r in json.load(open(observed_path, encoding="utf-8")) if r.get("distribution")]

    X, Y = [], []
    for r in recs:
        p = r["distribution"].get(r["reported"], 0.0)
        if p <= 0.0:
            continue
        X.append(logit(p))
        Y.append(1.0 if r["reported"] == r["expected"] else 0.0)

    n = len(X)
    if n < 20:
        sys.exit("need at least 20 usable rows to fit, got %d" % n)
    base = sum(Y) / n
    print("n = %d   base rate P(correct) = %.4f" % (n, base))

    def nll(a, b):
        tot = 0.0
        for x, y in zip(X, Y):
            z = a * x + b
            pz = 1.0 / (1.0 + math.exp(-z)) if z > -700 else 0.0
            tot -= y * math.log(max(pz, EPS)) + (1 - y) * math.log(max(1 - pz, EPS))
        return tot / n

    # Deterministic coordinate descent. NLL is convex in b for fixed a, and
    # effectively convex along this 2-parameter curve, so this converges.
    a, b, best = 1.0, 0.0, nll(1.0, 0.0)
    step_a = step_b = 2.0
    for _ in range(400):
        improved = False
        for da in (-step_a, step_a):
            v = nll(a + da, b)
            if v < best - 1e-12:
                a, best, improved = a + da, v, True
        for db in (-step_b, step_b):
            v = nll(a, b + db)
            if v < best - 1e-12:
                b, best, improved = b + db, v, True
        if not improved:
            step_a /= 2.0
            step_b /= 2.0
            if step_a < 1e-7:
                break
    print("platt: slope = %.4f  intercept = %.4f  NLL = %.4f" % (a, b, best))

    def calibrated(p):
        return 1.0 / (1.0 + math.exp(-(a * logit(p) + b)))

    # --- Honest reporting of how good the calibration actually is -------------
    pairs = [(calibrated(r["distribution"].get(r["reported"], 0.0) or EPS),
              1.0 if r["reported"] == r["expected"] else 0.0) for r in recs]
    raw = [(r.get("raw_confidence") or 0.0,
            1.0 if r["reported"] == r["expected"] else 0.0) for r in recs]

    def auc(pairs_):
        pos = [p for p, y in pairs_ if y == 1.0]
        neg = [p for p, y in pairs_ if y == 0.0]
        if not pos or not neg:
            return float("nan")
        return sum((p > q) + 0.5 * (p == q) for p in pos for q in neg) / (len(pos) * len(neg))

    auc_cal, auc_raw = auc(pairs), auc(raw)

    ece = 0.0
    bins = [[] for _ in range(10)]
    for p, y in pairs:
        bins[min(9, int(p * 10))].append((p, y))
    print("\nreliability of calibrated confidence:")
    for i, bn in enumerate(bins):
        if not bn:
            continue
        mp = sum(p for p, _ in bn) / len(bn)
        mo = sum(y for _, y in bn) / len(bn)
        ece += len(bn) / len(pairs) * abs(mp - mo)
        print("  %.1f-%.1f  n=%-4d predicted=%.3f empirical=%.3f"
              % (i / 10, (i + 1) / 10, len(bn), mp, mo))
    print("ECE = %.4f" % ece)
    print("AUC calibrated = %.4f   (raw reported confidence was %.4f)" % (auc_cal, auc_raw))

    s = sorted(p for p, _ in pairs)
    print("spread: p05=%.3f p50=%.3f p95=%.3f range=%.3f"
          % (s[n // 20], s[n // 2], s[n * 19 // 20], s[-1] - s[0]))

    artifact = {
        "_description": "IntentForge v2 - intent confidence calibration (Platt scaling). "
                        "Maps the engine's p(reported label) onto the empirically measured "
                        "probability that the reported label is correct. Fitted by "
                        "scripts/fit_intent_calibration.py against a labeled corpus; refit "
                        "to improve from experience. No per-query table.",
        "_method": "platt_scaling",
        "_feature": "logit(p(reported_label))",
        "slope": round(a, 6),
        "intercept": round(b, 6),
        "fit_n": n,
        "fit_base_rate": round(base, 6),
        "fit_ece": round(ece, 6),
        "fit_auc": round(auc_cal, 6),
        "fit_auc_raw_before_fix": round(auc_raw, 6),
    }
    json.dump(artifact, open(out_path, "w", encoding="utf-8"), indent=2)
    print("\nwrote " + out_path)
    print(json.dumps(artifact, indent=2))


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--collect", action="store_true", help="replay the corpus through the live engine")
    ap.add_argument("--fit", action="store_true", help="fit coefficients and write the artifact")
    ap.add_argument("--corpus", default="services/intent-engine/config/calibration_corpus.csv")
    ap.add_argument("--observed", default="/tmp/if_observed.json")
    ap.add_argument("--out", default="services/intent-engine/config/intent_calibration.json")
    ap.add_argument("--container", default=CONTAINER)
    a = ap.parse_args()
    if a.collect:
        collect(a.corpus, a.observed, a.container)
    if a.fit:
        fit(a.observed, a.out)
    if not (a.collect or a.fit):
        ap.error("pass --collect and/or --fit")


if __name__ == "__main__":
    main()
