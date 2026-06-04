import re
from typing import List, Dict

STOP_WORDS = {
    "the","a","an","is","and","or","for","to","in","on","of","with","support","model",
    "vs","like","than","other","instead"
}

def normalize(text: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()

def tokenize(text: str) -> List[str]:
    text = normalize(text)
    return [t for t in re.split(r"\s+", text) if t and t not in STOP_WORDS]

def extract_constraints(query: str):
    q = query.lower()
    positives = []
    negatives = []

    patterns = [
        (r"\bnot\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bwithout\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bexcept\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bbesides\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bexclu(ding|de)\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bno\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bminu?s\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\balternative\s+to\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\binstead\s+of\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"\bother\s+than\s+([a-z0-9][a-z0-9\-]*)", "prefix"),
        (r"-\s*([a-z0-9][a-z0-9\-]*)", "inline"),
    ]

    matched_spans = []

    for pat, kind in patterns:
        for m in re.finditer(pat, q):
            span = m.span()
            term = m.group(1)
            term = normalize(term)
            if not term:
                continue
            negatives.append(term)
            if kind == "prefix":
                matched_spans.append(span)

    # positives from non-stopwords outside the span that precede each negation operator
    # and from normalized remainder
    cleaned = q
    for span in sorted(matched_spans, reverse=True):
        cleaned = cleaned[: span[0]] + " " + cleaned[span[1] :]

    positives = [t for t in tokenize(cleaned) if t not in negatives]
    # de-duplicate while preserving order
    negatives = list(dict.fromkeys(negatives))
    positives = list(dict.fromkeys(positives))
    return {"positive": positives, "negative": negatives}

def normalize_constraint_terms(terms: List[str]) -> List[str]:
    return [normalize(t) for t in terms if normalize(t)]

def normalized_fields(result: Dict):
    title = normalize(result.get("title",""))
    content = normalize(result.get("content",""))
    url = normalize(result.get("url",""))
    return title + " " + content + " " + url

def should_exclude(result: Dict, negatives: List[str]) -> bool:
    blob = normalized_fields(result)
    token_blob = set(re.split(r"\s+", blob))
    for neg in negatives:
        # substring match OR token match
        if neg in token_blob or neg in blob:
            return False
    return True
    return False

def verify(results: List[Dict], constraints: Dict) -> Dict:
    positives = [normalize(t) for t in constraints.get("positive", [])]
    negatives = [normalize(t) for t in constraints.get("negative", [])]
    included = []
    excluded = []
    for r in results:
        blob = normalized_fields(r)
        excluded_flag = any(neg in blob for neg in negatives)
        entry = {
            "title": r.get("title"),
            "url": r.get("url"),
            "score": r.get("score"),
            "negative_match": excluded_flag,
        }
        (excluded if excluded_flag else included).append(entry)
    hits = {
        "positive_title": sum(any(t in normalize(r.get("title","")) for t in positives) for r in included),
        "positive_url": sum(any(t in normalize(r.get("url","")) for t in positives) for r in included),
    }
    misses = []
    for r in included:
        title_url = normalize(r.get("title","")) + " " + normalize(r.get("url",""))
        if not any(t in title_url for t in positives):
            misses.append(r["title"])
    return {
        "included_count": len(included),
        "excluded_count": len(excluded),
        "first_5_included": included[:5],
        "first_5_excluded": excluded[:5],
        "positive_hits": hits,
        "positive_misses_in_first_5": misses[:5],
    }
