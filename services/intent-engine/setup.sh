#!/bin/sh
set -e

MODEL_DIR="/app/models"

# MiniLM L6 v2 (for embeddings + intent classification)
EMBED_MODEL_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors"
EMBED_CONFIG_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json"
EMBED_TOKENIZER_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"

mkdir -p "$MODEL_DIR"

download_and_verify() {
    URL="$1"
    TARGET="$2"
    MIN_SIZE="$3"
    TMP="${TARGET}.tmp"

    if [ -f "$TARGET" ]; then
        SIZE=$(wc -c < "$TARGET" 2>/dev/null || echo 0)
        if [ "$SIZE" -ge "$MIN_SIZE" ]; then
            echo "File $TARGET exists and is valid (size $SIZE bytes >= $MIN_SIZE bytes)."
            return 0
        else
            echo "File $TARGET is corrupted/incomplete (size $SIZE < min $MIN_SIZE bytes). Removing..."
            rm -f "$TARGET"
        fi
    fi

    echo "Downloading $URL..."
    rm -f "$TMP"
    curl -fL --retry 5 --retry-delay 2 "$URL" -o "$TMP"
    TMP_SIZE=$(wc -c < "$TMP" 2>/dev/null || echo 0)
    if [ "$TMP_SIZE" -ge "$MIN_SIZE" ]; then
        mv "$TMP" "$TARGET"
        echo "Successfully downloaded $TARGET (size $TMP_SIZE bytes)."
    else
        echo "Error: Downloaded file $TMP size $TMP_SIZE is smaller than minimum $MIN_SIZE bytes!"
        rm -f "$TMP"
        exit 1
    fi
}

download_and_verify "$EMBED_MODEL_URL" "$MODEL_DIR/model.safetensors" 85000000
download_and_verify "$EMBED_CONFIG_URL" "$MODEL_DIR/config.json" 100
download_and_verify "$EMBED_TOKENIZER_URL" "$MODEL_DIR/tokenizer_embed.json" 100000

# Clean up old Qwen models if present (save ~400MB)
rm -f "$MODEL_DIR/qwen2.5-0.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/tokenizer.json"

echo "Starting Intent Engine (lightweight mode: rules + centroids)..."
exec ./intent-engine
