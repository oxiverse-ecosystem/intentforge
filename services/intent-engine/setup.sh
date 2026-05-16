#!/bin/sh
set -e

MODEL_DIR="/app/models"
# Qwen 1.5B
MODEL_FILE="$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
MODEL_URL="https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf"
TOKENIZER_URL="https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct/resolve/main/tokenizer.json"

# MiniLM L6 v2 (for embeddings)
EMBED_MODEL_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/model.safetensors"
EMBED_CONFIG_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/config.json"
EMBED_TOKENIZER_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json"

mkdir -p "$MODEL_DIR"

if [ ! -f "$MODEL_FILE" ]; then
    echo "Downloading Qwen model..."
    curl -L "$MODEL_URL" -o "$MODEL_FILE"
fi

if [ ! -f "$MODEL_DIR/tokenizer.json" ]; then
    echo "Downloading Qwen tokenizer..."
    curl -L "$TOKENIZER_URL" -o "$MODEL_DIR/tokenizer.json"
fi

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

echo "Starting Intent Engine..."
exec ./intent-engine
