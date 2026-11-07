#!/bin/sh
set -e

MODEL_DIR="/app/models"

# MiniLM L6 v2 (for embeddings + intent classification). All artifacts share a
# pinned revision; SHA-256 values are verified before reuse or promotion.
EMBED_REVISION="1110a243fdf4706b3f48f1d95db1a4f5529b4d41"
EMBED_MODEL_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/${EMBED_REVISION}/model.safetensors"
EMBED_CONFIG_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/${EMBED_REVISION}/config.json"
EMBED_TOKENIZER_URL="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/${EMBED_REVISION}/tokenizer.json"

mkdir -p "$MODEL_DIR"

verify_file() {
    [ -f "$1" ] && printf '%s  %s\n' "$2" "$1" | sha256sum -c - >/dev/null 2>&1
}

download_and_verify() {
    URL="$1"
    TARGET="$2"
    EXPECTED_SHA256="$3"
    TMP="${TARGET}.tmp"

    if verify_file "$TARGET" "$EXPECTED_SHA256"; then
        echo "Verified existing $TARGET."
        return 0
    fi
    rm -f "$TARGET"

    if verify_file "$TMP" "$EXPECTED_SHA256"; then
        mv "$TMP" "$TARGET"
        return 0
    fi
    # An unverified partial may belong to an older revision. Start clean.
    rm -f "$TMP"
    ATTEMPT=0
    while [ "$ATTEMPT" -lt 2 ]; do
        ATTEMPT=$((ATTEMPT + 1))
        echo "Downloading $URL..."
        if curl -fL -C - --retry 5 --retry-delay 2 "$URL" -o "$TMP"; then
            :
        else
            CURL_STATUS=$?
            if [ "$CURL_STATUS" -eq 33 ]; then
                # Server ignored Range: curl cannot restart this transfer itself.
                rm -f "$TMP"
                curl -fL --retry 5 --retry-delay 2 "$URL" -o "$TMP" || return $?
            else
                return "$CURL_STATUS"
            fi
        fi
        if verify_file "$TMP" "$EXPECTED_SHA256"; then
            mv "$TMP" "$TARGET"
            echo "Verified and installed $TARGET."
            return 0
        fi
        echo "Checksum mismatch for $TMP; discarding download." >&2
        rm -f "$TMP"
    done
    echo "Error: unable to download verified $TARGET." >&2
    return 1
}

download_and_verify "$EMBED_MODEL_URL" "$MODEL_DIR/model.safetensors" 53aa51172d142c89d9012cce15ae4d6cc0ca6895895114379cacb4fab128d9db
download_and_verify "$EMBED_CONFIG_URL" "$MODEL_DIR/config.json" 953f9c0d463486b10a6871cc2fd59f223b2c70184f49815e7efbcab5d8908b41
download_and_verify "$EMBED_TOKENIZER_URL" "$MODEL_DIR/tokenizer_embed.json" be50c3628f2bf5bb5e3a7f17b1f74611b2561a3a27eeab05e5aa30f411572037

# Clean up old Qwen models if present (save ~400MB)
rm -f "$MODEL_DIR/qwen2.5-0.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/qwen2.5-1.5b-instruct-q4_k_m.gguf"
rm -f "$MODEL_DIR/tokenizer.json"

echo "Starting Intent Engine (lightweight mode: rules + centroids)..."
exec ./intent-engine
