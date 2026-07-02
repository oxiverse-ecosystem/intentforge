#!/usr/bin/env python3
"""
Generate dictionary.rs for the spelling corrector from Norvig's count_1w.txt
(Google Books Ngram frequency data) plus curated tech/misspelling entries.

Usage:
    python3 scripts/generate_dictionary.py
    # Reads data/count_1w.txt, outputs services/gateway/src/dictionary.rs
"""

import math
import os

# Paths
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
INPUT_FILE = os.path.join(PROJECT_ROOT, "data", "count_1w.txt")
OUTPUT_FILE = os.path.join(PROJECT_ROOT, "services", "gateway", "src", "dictionary.rs")

# How many top words to include from the Ngram data
TOP_N = 15_000

# ── Curated tech terms (these always get included) ──
# These may not appear in Google Books Ngram data, or may have low counts there,
# but are critical for search queries.
TECH_TERMS = {
    # Languages & frameworks
    "rust": 0.45, "python": 0.44, "javascript": 0.42, "typescript": 0.40,
    "java": 0.39, "golang": 0.37, "react": 0.36, "vue": 0.35, "angular": 0.34,
    "svelte": 0.33, "nextjs": 0.32, "nodejs": 0.30, "deno": 0.29, "bun": 0.28,
    "django": 0.30, "flask": 0.29, "fastapi": 0.28, "rails": 0.28,
    "spring": 0.27, "laravel": 0.26,

    # DevOps & infra
    "docker": 0.35, "kubernetes": 0.34, "k8s": 0.33, "nginx": 0.32,
    "apache": 0.32, "swarm": 0.24, "redis": 0.31, "postgres": 0.30,
    "postgresql": 0.29, "mysql": 0.28, "mongodb": 0.27, "sqlite": 0.26,
    "linux": 0.30, "ubuntu": 0.29, "debian": 0.28, "alpine": 0.27,
    "windows": 0.29, "macos": 0.28, "android": 0.27, "ios": 0.26,
    "aws": 0.32, "gcp": 0.31, "azure": 0.30, "cloud": 0.28,
    "terraform": 0.26, "ansible": 0.25, "helm": 0.22, "ingress": 0.24,
    "compose": 0.25, "distro": 0.20,

    # Protocols & tools
    "api": 0.32, "sdk": 0.30, "cli": 0.29, "gui": 0.28,
    "http": 0.30, "https": 0.29, "tcp": 0.27, "udp": 0.26, "dns": 0.27, "ssh": 0.26,
    "json": 0.30, "xml": 0.25, "yaml": 0.24, "toml": 0.23,
    "git": 0.32, "github": 0.31, "gitlab": 0.30, "cargo": 0.30,
    "npm": 0.31, "yarn": 0.30, "pnpm": 0.29, "pip": 0.30,
    "webpack": 0.30, "vite": 0.29, "esbuild": 0.28,
    "tailwind": 0.30, "bootstrap": 0.29, "sass": 0.28, "css": 0.29, "html": 0.30,
    "gnome": 0.20, "emacs": 0.28, "vim": 0.27, "neovim": 0.23,
    "crate": 0.18, "schema": 0.18, "parser": 0.16, "config": 0.20,
    "registry": 0.14, "extension": 0.16, "extensions": 0.14,
    "editor": 0.18, "prod": 0.22, "podman": 0.22,
}

# ── Common misspellings / confused words (kept at 0.001 to trigger SymSpell) ──
MISSPELLINGS = {
    "recieve": 0.001, "acheive": 0.001, "definately": 0.001,
    "seperate": 0.001, "occured": 0.001, "calender": 0.001,
    "neccessary": 0.001, "embarass": 0.001, "goverment": 0.001,
    "enviorment": 0.001, "recieving": 0.001, "acheiving": 0.001,
    "begginer": 0.001, "begginers": 0.001, "alternitiv": 0.001,
    "programing": 0.001, "programed": 0.001,
    "framwork": 0.001, "frameworks": 0.001, "languge": 0.001,
    "libary": 0.001, "libaries": 0.001,
    "deploymint": 0.001, "deply": 0.001, "depoly": 0.001,
    "perfomance": 0.001, "perfom": 0.001,
    "editer": 0.001, "begginners": 0.001, "alternitiv": 0.001,
    "orcale": 0.001, "agular": 0.001, "pypeline": 0.001,
    "surprize": 0.001,
}

# ── Additional common English words not caught by Ngram data ──
# These are words that should be in the dictionary but might not appear
# in top 15k of Google Books Ngrams (or appear with low frequency)
ADDITIONAL_WORDS = {
    "app": 0.20, "apps": 0.18,
    "alternative": 0.16, "alternatives": 0.14,
    "beginner": 0.14, "beginners": 0.12, "best": 0.24,
    # Words commonly searched for but below top-15k in Ngram corpus
    "embarrass": 0.010, "embarrassed": 0.010, "embarrassing": 0.010,
    "embarrassment": 0.010, "embarrasses": 0.010,
    "minuscule": 0.010, "minuscule": 0.010,
    "allotment": 0.010, "publicly": 0.010,
    "privilege": 0.010, "privileges": 0.010,
    "occurrence": 0.010, "occurrences": 0.010,
    "committee": 0.010, "committees": 0.010,
    "recommend": 0.010, "recommends": 0.010, "recommended": 0.010, "recommending": 0.010,
    "calendar": 0.010, "calendars": 0.010,
    "necessary": 0.010, "necessarily": 0.010,
    "separate": 0.010, "separately": 0.010, "separated": 0.010,
    "definitely": 0.010,
    "independent": 0.010, "independence": 0.010,
    "environment": 0.010, "environments": 0.010, "environmental": 0.010,
    "government": 0.010, "governments": 0.010, "governmental": 0.010,
    "millennium": 0.010,
    "liaison": 0.010,
    "parallel": 0.010,
    "accommodate": 0.010, "accommodation": 0.010,
    "harass": 0.010, "harassed": 0.010, "harassment": 0.010,
    "receive": 0.010, "receives": 0.010, "received": 0.010, "receiving": 0.010,
    "achieve": 0.010, "achieves": 0.010, "achieved": 0.010, "achieving": 0.010, "achievement": 0.010,
    "weird": 0.010,
    "rhythm": 0.010,
    "tomorrow": 0.010,
    "until": 0.010,
    "truly": 0.010,
    "misspell": 0.010, "misspelled": 0.010, "misspelling": 0.010, "misspellings": 0.010,
    "surprise": 0.010, "surprised": 0.010, "surprising": 0.010,
    "successful": 0.010, "successfully": 0.010,
    "writing": 0.010,
    "occasion": 0.010, "occasions": 0.010, "occasional": 0.010,
    "apparent": 0.010, "apparently": 0.010,
    "acquire": 0.010, "acquired": 0.010, "acquiring": 0.010,
    "refer": 0.010, "referred": 0.010, "referring": 0.010, "reference": 0.010, "references": 0.010,
    "remember": 0.010, "remembered": 0.010, "remembers": 0.010,
    "curriculum": 0.010, "curricula": 0.010,
    "environment": 0.010, "environments": 0.010,

    # Words that don't appear in Google Books Ngram corpus but are
    # legitimate English words commonly used in search queries
    "async": 0.010,
    "concurrency": 0.010, "concurrent": 0.010,
    "zonal": 0.010,
    "skillet": 0.010,
    "migrating": 0.010, "migrated": 0.010, "migrates": 0.010,
    "spreadsheet": 0.010, "spreadsheets": 0.010,
}


def load_ngram_data(filepath: str, top_n: int) -> list[tuple[str, int]]:
    """Load the top N words from count_1w.txt by raw count."""
    entries = []
    with open(filepath, "r", encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line or "\t" not in line:
                continue
            parts = line.split("\t")
            if len(parts) != 2:
                continue
            word, count_str = parts
            # Only lowercase, alphabetic words
            if not word.isalpha():
                continue
            word_lower = word.lower()
            try:
                count = int(count_str)
            except ValueError:
                continue
            entries.append((word_lower, count))
    # Sort by count descending, take top N
    entries.sort(key=lambda x: -x[1])
    return entries[:top_n]


def scale_frequency(raw_count: int, max_log: float, min_log: float) -> float:
    """Scale raw count to frequency in [0.001, 1.0] using log scaling."""
    log_count = math.log10(raw_count + 1)
    if max_log == min_log:
        return 1.0
    # Normalize to [0, 1]
    normalized = (log_count - min_log) / (max_log - min_log)
    # Scale to [0.001, 1.0]
    return 0.001 + 0.999 * normalized


def generate_dictionary(input_path: str, output_path: str, top_n: int):
    """Generate dictionary.rs from Ngram data + curated entries."""

    # Load Ngram data
    print(f"Loading top {top_n} words from {input_path}...")
    ngram_words = load_ngram_data(input_path, top_n)
    print(f"  Loaded {len(ngram_words)} words")

    # Determine frequency scaling bounds
    max_count = ngram_words[0][1] if ngram_words else 1
    min_count = ngram_words[-1][1] if ngram_words else 1
    max_log = math.log10(max_count + 1)
    min_log = math.log10(min_count + 1)
    print(f"  Count range: {min_count:,} - {max_count:,}")
    print(f"  Log range: {min_log:.2f} - {max_log:.2f}")

    # Build frequency dictionary from Ngram data (scaled)
    freq_dict: dict[str, float] = {}
    for word, count in ngram_words:
        freq = scale_frequency(count, max_log, min_log)
        freq_dict[word] = freq

    # Merge curated tech terms (override with higher frequency if tech term known)
    for word, freq in TECH_TERMS.items():
        word_lower = word.lower()
        freq_dict[word_lower] = max(freq_dict.get(word_lower, 0), freq)

    # Merge additional words
    for word, freq in ADDITIONAL_WORDS.items():
        word_lower = word.lower()
        freq_dict[word_lower] = max(freq_dict.get(word_lower, 0), freq)

    # Add misspellings (always at 0.001)
    for word, freq in MISSPELLINGS.items():
        freq_dict[word.lower()] = freq

    # Sort by frequency descending for output
    sorted_words = sorted(freq_dict.items(), key=lambda x: -x[1])

    print(f"  Total unique words: {len(sorted_words)}")

    # Generate dictionary.rs
    lines = [
        '// ─── Embedded English Word Frequency Dictionary ──────────────────────',
        '// Auto-generated by scripts/generate_dictionary.py from Norvig\'s',
        '// count_1w.txt (Google Web Trillion Word Corpus).',
        '//',
        '// Scale: 1.0 (most common, e.g. "the") -> 0.01 (valid words).',
        '// Entries below 0.01 (e.g. misspelling forms at 0.001) are treated as',
        '// intentional misspellings and fall through to SymSpell for correction.',
        '',
        'pub(crate) const WORD_FREQUENCIES: &[(&str, f64)] = &[',
    ]

    # Output in chunks of 8 per line for readability
    entries_per_line = 8
    for i in range(0, len(sorted_words), entries_per_line):
        chunk = sorted_words[i:i + entries_per_line]
        entry_strs = []
        for word, freq in chunk:
            # Format frequency: remove trailing zeros, keep 3-6 significant digits
            if freq >= 0.01:
                freq_str = f"{freq:.3f}"
            elif freq >= 0.001:
                freq_str = f"{freq:.4f}"
            else:
                freq_str = f"{freq:.6f}"
            entry_strs.append(f'("{word}", {freq_str})')
        lines.append("    " + ", ".join(entry_strs) + ",")

    lines.append("];\n")

    # Write output
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print(f"\nDictionary written to {output_path}")
    print(f"  {len(sorted_words)} total words")


if __name__ == "__main__":
    generate_dictionary(INPUT_FILE, OUTPUT_FILE, TOP_N)
