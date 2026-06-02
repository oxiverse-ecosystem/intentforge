import json, csv, os, subprocess
import numpy as np
from collections import Counter
from sklearn.linear_model import LogisticRegression

CSV_PATH = os.path.join(os.path.dirname(__file__), "..", "..", "calibration_benchmark_200.csv")
OUTPUT_PATH = os.path.join(os.path.dirname(__file__), "config", "intent_weights.json")

rows = list(csv.DictReader(open(CSV_PATH)))
queries = [r["query"] for r in rows]
expected = [r["expected"] for r in rows]
print(f"Loaded {len(queries)} queries")

counts = Counter(expected)
for lbl in ["navigational", "informational", "technical", "how-to", "comparison", "transactional", "fresh"]:
    print(f"  {lbl}: {counts.get(lbl, 0)}")

# Compute embeddings via the actual Candle /embed endpoint
print("Fetching embeddings from container (candle /embed)...")
embeddings = []
for i, q in enumerate(queries):
    import urllib.request
    import urllib.parse
    url = "http://localhost:3005/embed?text=" + urllib.parse.quote(q)
    try:
        resp = urllib.request.urlopen(url, timeout=30)
        data = json.loads(resp.read())
        emb = data["embedding"]
        if len(emb) != 384:
            print(f"  WARNING: query {i} has embedding dim {len(emb)}, expected 384")
            emb = np.zeros(384)
        embeddings.append(emb)
    except Exception as e:
        print(f"  WARNING: failed to fetch embedding for query {i} ({q}): {e}")
        embeddings.append(np.zeros(384))
    if (i+1) % 25 == 0:
        print(f"  {i+1}/{len(queries)} done")

embeddings = np.array(embeddings)
print(f"Embeddings shape: {embeddings.shape}")

# Validate some norms
norms = np.linalg.norm(embeddings, axis=1)
print(f"Embedding norms: min={norms.min():.4f}, max={norms.max():.4f}, mean={norms.mean():.4f}")

# Train logistic regression
print("Training multinomial logistic regression...")
C_values = [0.01, 0.05, 0.1, 0.5, 1.0, 5.0, 10.0, 100.0]
from sklearn.model_selection import cross_val_score, StratifiedKFold

skf = StratifiedKFold(n_splits=5, shuffle=True, random_state=42)
best_score = 0.0
best_C = None

for C in C_values:
    clf = LogisticRegression(solver="lbfgs", C=C, max_iter=5000, random_state=42)
    scores = cross_val_score(clf, embeddings, expected, cv=skf, scoring="accuracy")
    mean = scores.mean()
    print(f"  C={C:.2f}: {mean:.4f} (std={scores.std():.4f})")
    if mean > best_score:
        best_score = mean
        best_C = C

print(f"\nBest C={best_C:.2f} with CV accuracy={best_score:.4f}")

clf = LogisticRegression(solver="lbfgs", C=best_C, max_iter=5000, random_state=42)
clf.fit(embeddings, expected)

preds = clf.predict(embeddings)
train_acc = (preds == expected).mean()
print(f"Training accuracy: {train_acc:.4f} ({int(train_acc*len(queries))}/{len(queries)})")

# Show mistakes
print("\nMistakes:")
wrong = 0
for i, (q, exp, pred) in enumerate(zip(queries, expected, preds)):
    if pred != exp:
        wrong += 1
        proba = clf.predict_proba(embeddings[i:i+1])[0]
        conf = max(proba)
        print(f"  {q:<45} expected={exp:<15} predicted={pred:<15} conf={conf:.3f}")
print(f"\nAccuracy: {len(queries)-wrong}/{len(queries)} = {(len(queries)-wrong)/len(queries)*100:.1f}%")

# Export
output = {
    "_description": "IntentForge v2 - Linear Probe Weights (trained on Candle embeddings)",
    "_version": 3,
    "labels": list(clf.classes_),
    "weights": clf.coef_.tolist(),
    "bias": clf.intercept_.tolist(),
    "temperature": 0.05,
    "confidence": {"base": 0.3, "margin_multiplier": 0.7},
}
os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
with open(OUTPUT_PATH, "w") as f:
    json.dump(output, f, indent=2)
print(f"\nWeights exported to {OUTPUT_PATH}")
