"""Run calibration benchmark against the running Docker container's /analyze endpoint.
"""

import io
import json
import subprocess
import sys
from urllib.parse import quote

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding='utf-8')

CSV_PATH = "../../calibration_benchmark_200.csv"
CONTAINER = "if-dev-intent-engine"


def classify(query: str) -> dict | None:
    encoded = quote(query, safe="")
    url = f"http://localhost:3005/analyze?q={encoded}"
    try:
        result = subprocess.run(
            ["docker", "exec", CONTAINER, "curl", "-s", url],
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode != 0:
            return None
        return json.loads(result.stdout.strip())
    except Exception as e:
        print(f"  Error: {e}", file=sys.stderr)
        return None


def main():
    # Read CSV
    rows = []
    with open(CSV_PATH, encoding="utf-8") as f:
        lines = f.readlines()

    for line in lines[1:]:
        line = line.strip()
        if not line:
            continue
        parts = line.split(",")
        if len(parts) < 2:
            continue
        query = parts[0]
        expected = parts[1]
        rows.append((query, expected))

    print(f"Loaded {len(rows)} queries from {CSV_PATH}\n")

    correct = 0
    total = 0
    details = []

    for i, (query, expected) in enumerate(rows):
        result = classify(query)
        if result is None:
            details.append((query, expected, "ERROR", 0.0, False))
            continue

        predicted = result.get("intent", "?")
        confidence = result.get("confidence", 0.0)
        is_correct = predicted == expected
        if is_correct:
            correct += 1
        total += 1
        details.append((query, expected, predicted, confidence, is_correct))

        mark = "OK" if is_correct else "XX"
        print(f"[{i+1}/{len(rows)}] {mark} {query} -> {predicted} (expected={expected}, conf={confidence:.3f})")

    print(f"\n{'='*60}")
    print(f"RESULTS: {correct}/{total} correct ({correct/total*100:.1f}%)")
    print(f"{'='*60}")

    # Break down by label
    by_label = {}
    for query, expected, predicted, confidence, is_correct in details:
        by_label.setdefault(expected, {"total": 0, "correct": 0, "wrong": []})
        by_label[expected]["total"] += 1
        if is_correct:
            by_label[expected]["correct"] += 1
        else:
            by_label[expected]["wrong"].append((query, predicted, confidence))

    print("\nPer-label accuracy:")
    for label in sorted(by_label.keys()):
        info = by_label[label]
        acc = info["correct"] / info["total"] * 100
        print(f"  {label}: {info['correct']}/{info['total']} ({acc:.1f}%)")
        for query, predicted, conf in info["wrong"]:
            print(f"    WRONG: '{query}' -> {predicted} (conf={conf:.3f})")


if __name__ == "__main__":
    main()
