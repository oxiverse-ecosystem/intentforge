#!/bin/sh
set -e

MODEL_DIR="/app/models"

# MiniLM L6 v2 (for embeddings + intent classification)
EMBED_MODEL_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors"
EMBED_CONFIG_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json"
EMBED_TOKENIZER_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"

mkdir -p "$MODEL_DIR"

if [ ! -f "$MODEL_DIR/model.safetensors" ]; then
    echo "Downloading MiniLM model..."
    curl -L "$EMBED_MODEL_URL" -o "$MODEL_DIR/model.safetensors"
fi

if [ ! -f "$MODEL_DIR/config.json" ]; then
    echo "Downloading MiniLM config..."
    curl -L "$EMBED_CONFIG_URL" -o "$MODEL_DIR/config.json"
fi

if [ ! -f "$MODEL_DIR/tokenizer_embed.json" ]; then
    echo "Downloading MiniLM tokenizer..."
    curl -L "$EMBED_TOKENIZER_URL" -o "$MODEL_DIR/tokenizer_embed.json"
fi

# Clean up old Qwen models if present (save ~400MB)
rm -f "$MODEL_DIR/qwen2.5-0.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/tokenizer.json"

echo "Starting Intent Engine (lightweight mode: rules + centroids)..."
exec ./intent-engine
