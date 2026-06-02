"""Retrain linear probe using Candle embeddings from the running container.

Loads candle_embeddings.json, trains LogisticRegression, 
compares accuracy, and exports intent_weights.json (v4).
"""

import json
import numpy as np
from sklearn.linear_model import LogisticRegression
from sklearn.metrics import accuracy_score, classification_report

EMBEDDINGS_PATH = "candle_embeddings.json"
OLD_WEIGHTS_PATH = "config/intent_weights.json"
OUTPUT_PATH = "config/intent_weights.json"


def load_embeddings(path: str):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    X = []
    y = []
    for item in data:
        X.append(item["embedding"])
        y.append(item["expected"])
    return np.array(X, dtype=np.float64), np.array(y)


def train_probe(X, y):
    print(f"Training on {X.shape[0]} samples, {X.shape[1]} features")
    classes = sorted(set(y))
    print(f"Classes ({len(classes)}): {classes}")

    model = LogisticRegression(
        C=10.0,
        penalty="l2",
        solver="lbfgs",
        max_iter=5000,
        random_state=42,
        class_weight="balanced",
    )
    model.fit(X, y)

    y_pred = model.predict(X)
    train_acc = accuracy_score(y, y_pred)
    print(f"\nTraining accuracy: {train_acc:.4f} ({train_acc*100:.1f}%)")
    class_labels = list(model.classes_)
    print(f"Class order in model: {class_labels}")
    print(f"\nClassification report:\n{classification_report(y, y_pred)}")

    return model


def export_weights(model, output_path):
    weights = model.coef_.astype(np.float32)
    bias = model.intercept_.astype(np.float32)

    # Preserve confidence params from old config
    temperature = 1.0
    confidence_base = 0.3
    margin_multiplier = 0.8
    try:
        with open(OLD_WEIGHTS_PATH) as f:
            old = json.load(f)
        temperature = old.get("temperature", 1.0)
        conf = old.get("confidence", {})
        confidence_base = conf.get("base", 0.3)
        margin_multiplier = conf.get("margin_multiplier", 0.8)
    except Exception:
        pass

    data = {
        "_description": "IntentForge v2 - Linear Probe Weights (trained on Candle embeddings)",
        "_version": 4,
        "labels": list(model.classes_),
        "weights": weights.tolist(),
        "bias": bias.tolist(),
        "temperature": temperature,
        "confidence": {
            "base": confidence_base,
            "margin_multiplier": margin_multiplier,
        },
    }

    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)

    print(f"\nExported weights: {output_path}")
    print(f"  Weight matrix: {weights.shape} ({weights.dtype})")
    print(f"  Bias vector: {bias.shape}")
    for i, label in enumerate(model.classes_):
        print(f"  {label}: weight_norm={np.linalg.norm(weights[i]):.4f}, bias={bias[i]:.4f}")


def compare_old_new(X, y, old_path):
    """Compare accuracy of old (sentence-transformers) vs new (Candle) weights."""
    try:
        with open(old_path) as f:
            old = json.load(f)
        W_old = np.array(old["weights"], dtype=np.float64)
        b_old = np.array(old["bias"], dtype=np.float64)
        old_labels = old.get("labels", [f"c{i}" for i in range(W_old.shape[0])])
        logits = X @ W_old.T + b_old
        y_pred = np.array(old_labels)[logits.argmax(axis=1)]
        old_acc = accuracy_score(y, y_pred)
        print(f"\nOld weights (trained on sentence-transformers, labels={old_labels}) accuracy on Candle embeddings: {old_acc:.4f} ({old_acc*100:.1f}%)")
    except Exception as e:
        print(f"Could not compare old weights: {e}")


def main():
    print("=" * 60)
    print("Retraining linear probe on Candle embeddings")
    print("=" * 60)

    X, y = load_embeddings(EMBEDDINGS_PATH)
    print(f"\nLoaded {len(X)} Candle embeddings")

    compare_old_new(X, y, OLD_WEIGHTS_PATH)

    model = train_probe(X, y)
    export_weights(model, OUTPUT_PATH)

    print(f"\nDone. New weights exported to {OUTPUT_PATH}")


if __name__ == "__main__":
    main()
