#!/usr/bin/env python3
"""Analyze the crawler's raw content-length distribution from debug logs.

Parses `Content length: raw=N truncated=M cap=C` lines from `docker logs
if-dev-crawler`, prints the raw-length distribution (p50/p75/p90/p95/p99),
and models the stored-content footprint under candidate CONTENT_CAP_MAX
values so the clamp can be justified from measured data instead of inertia.
"""
import re
import subprocess
import sys

def main():
    out = subprocess.run(
        ["docker", "logs", "if-dev-crawler"],
        capture_output=True, text=True
    )
    text = out.stdout + out.stderr
    pat = re.compile(r"Content length: raw=(\d+) truncated=(\d+) cap=(\d+)")
    samples = [(int(a), int(b), int(c)) for a, b, c in pat.findall(text)]
    if not samples:
        print("No samples found — is RUST_LOG=info,crawler=debug set?")
        sys.exit(1)

    raws = sorted(s[0] for s in samples)
    n = len(raws)

    def pct(p):
        return raws[min(int(n * p / 100), n - 1)]

    print(f"samples: {n}")
    print(f"raw length distribution (chars):")
    for p in (50, 75, 90, 95, 99):
        print(f"  p{p}: {pct(p)}")
    print(f"  max: {raws[-1]}")
    mean = sum(raws) / n
    print(f"  mean: {mean:.0f}")

    total_raw = sum(raws)
    print(f"\nstored-content footprint model (chars actually indexed under cap):")
    print(f"  {'cap':>7} {'stored':>14} {'vs uncapped':>11} {'vs cap=12000':>12} {'pages truncated':>15}")
    base12 = sum(min(r, 12000) for r in raws)
    for cap in (4000, 5000, 6000, 8000, 10000, 12000, 16000, None):
        if cap is None:
            stored = total_raw
            label = "uncap"
            trunc = 0
        else:
            stored = sum(min(r, cap) for r in raws)
            label = str(cap)
            trunc = sum(1 for r in raws if r > cap)
        print(f"  {label:>7} {stored:>14,} {stored/total_raw:>10.1%} {stored/base12:>11.1%} {trunc:>8} ({trunc/n:.0%})")

    # Where does the volume live? Contribution of the tail.
    print(f"\ntail contribution (share of total raw chars):")
    for cut in (8000, 12000, 20000):
        tail_chars = sum(r - cut for r in raws if r > cut)
        print(f"  chars beyond {cut}: {tail_chars:,} ({tail_chars/total_raw:.1%} of corpus)")

if __name__ == "__main__":
    main()
