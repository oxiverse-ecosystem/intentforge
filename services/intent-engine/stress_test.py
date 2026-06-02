import json, subprocess, time, urllib.parse, sys

GATEWAY = "http://localhost:4000"
RESULTS_OUT = "stress_test_results.json"

queries = [
    # ─── Navigational ───
    ("navigational", "reddit"),
    ("navigational", "github login"),
    ("navigational", "gmail sign in"),
    ("navigational", "aws console"),
    ("navigational", "youtube music"),
    ("navigational", "stackoverflow"),
    ("navigational", "hacker news"),
    ("navigational", "tailwind css docs"),

    # ─── Informational ───
    ("informational", "what is quantum computing"),
    ("informational", "how does photosynthesis work"),
    ("informational", "why is the sky blue"),
    ("informational", "what is transformer architecture"),
    ("informational", "explain relativity theory"),
    ("informational", "who invented python programming language"),
    ("informational", "what causes earthquakes"),
    ("informational", "meaning of life"),

    # ─── How-To ───
    ("how-to", "how to tie a tie step by step"),
    ("how-to", "how to deploy kubernetes cluster on aws"),
    ("how-to", "how to implement oauth2 authentication"),
    ("how-to", "how to reverse engineer an apk file"),
    ("how-to", "how to configure nginx reverse proxy"),
    ("how-to", "how to optimize postgresql query performance"),
    ("how-to", "how to build a rest api with axum"),
    ("how-to", "how to write unit tests in python"),

    # ─── Transactional ───
    ("transactional", "buy iphone 16 pro max"),
    ("transactional", "download android studio"),
    ("transactional", "subscribe to netflix premium"),
    ("transactional", "book flight to london cheap"),
    ("transactional", "purchase github copilot pro"),
    ("transactional", "sign up for chatgpt plus"),
    ("transactional", "rent a car in tokyo japan"),
    ("transactional", "order pizza online delivery"),

    # ─── Comparison ───
    ("comparison", "rust vs go performance 2026"),
    ("comparison", "react vs vue vs angular 2026"),
    ("comparison", "aws vs gcp vs azure pricing"),
    ("comparison", "postgresql vs mysql vs mongodb"),
    ("comparison", "docker vs podman vs containerd"),
    ("comparison", "typescript vs javascript for web development"),
    ("comparison", "mac vs linux for programming"),
    ("comparison", "svelte vs react vs solidjs"),

    # ─── Technical ───
    ("technical", "rust async runtime tokio vs async-std"),
    ("technical", "python asyncio event loop internals"),
    ("technical", "kubernetes ingress controller setup"),
    ("technical", "redis caching strategy patterns"),
    ("technical", "nginx reverse proxy load balancing"),
    ("technical", "docker compose networking multi-service"),
    ("technical", "css grid layout responsive design"),
    ("technical", "linux kernel module development"),

    # ─── Fresh / Local ───
    ("fresh", "weather today"),
    ("fresh", "weather in new york"),

    # ─── Multi-Constraint ───
    ("multi-constraint", "python async await with database postgresql"),
    ("multi-constraint", "react hooks typescript functional components"),
    ("multi-constraint", "docker compose networking nginx postgres redis"),
    ("multi-constraint", "terraform aws ec2 s3 rds setup"),
    ("multi-constraint", "kubernetes ingress tls cert-manager letsencrypt"),

    # ─── Negative Constraints ───
    ("negative", "javascript frameworks without react"),
    ("negative", "programming languages not python"),
    ("negative", "search engines except google"),
    ("negative", "linux distros without systemd"),
    ("negative", "alternative to notion aws"),
    ("negative", "better than docker for development"),
    ("negative", "text editors except vscode"),
    ("negative", "cloud providers excluding aws"),

    # ─── Complex / Edge Cases ───
    ("complex", "how do i deploy a django react postgres app on aws ecs with ci cd pipeline github actions"),
    ("complex", "what is the best way to learn system design and distributed systems in 2026"),
    ("complex", "compare rust actix vs axum vs tide for building high performance microservices"),
    ("complex", "troubleshoot nginx 502 bad gateway with php fpm timeout issues"),
    ("complex", "docker permission denied trying to connect to the docker daemon socket"),
    ("complex", "how to migrate from javascript to typescript in a large react codebase"),
    ("complex", "set up elasticsearch logstash kibana elk stack with docker compose"),
    ("complex", "buy macbook pro m4 vs dell xps 16 for machine learning development"),
]


def search(query):
    encoded = urllib.parse.quote(query, safe="")
    url = f"{GATEWAY}/search?q={encoded}"
    start = time.time()
    try:
        result = subprocess.run(
            ["curl", "-s", "-m", "30", url],
            capture_output=True, text=True, timeout=35,
        )
        elapsed = time.time() - start
        if result.returncode != 0:
            return {"error": f"curl failed (rc={result.returncode})", "latency": elapsed}
        data = json.loads(result.stdout.strip())
        data["_latency"] = round(elapsed, 3)
        return data
    except Exception as e:
        return {"error": str(e), "latency": time.time() - start}


def analyze_result(qtype, label, query, resp):
    if "error" in resp:
        return f"  ERROR: {resp['error']}"

    intent = resp.get("intent", "?")
    confidence = resp.get("confidence", 0)
    constraints = resp.get("constraints", [])
    structured = resp.get("structured_constraints", {})
    expanded = resp.get("expanded_queries", [])
    results = resp.get("results", [])
    latency = resp.get("_latency", 0)

    # Check negative constraints
    neg = structured.get("negative", [])
    entities = structured.get("entities", [])
    entity_texts = [(e.get("text",""), e.get("role","")) for e in entities]

    # Top results info
    top_sources = []
    top_scores = []
    for r in results[:3]:
        top_sources.extend(r.get("sources", []))
        top_scores.append(round(r.get("score", 0), 3))

    lines = []
    lines.append(f"  Intent: {intent} ({confidence:.2f}) — expected: {label}")
    lines.append(f"  Latency: {latency:.2f}s | Results: {len(results)} | Expanded: {len(expanded)}")
    if constraints:
        lines.append(f"  Constraints: {constraints[:6]}{'...' if len(constraints)>6 else ''}")
    if neg:
        lines.append(f"  NEGATIVE: {neg}")
    if entity_texts:
        roles = ", ".join(f"'{t}'→{r}" for t, r in entity_texts[:4])
        lines.append(f"  Entities: {roles}")
    if top_scores:
        lines.append(f"  Top scores: {top_scores}")
    if top_sources:
        unique_sources = list(dict.fromkeys(top_sources))
        lines.append(f"  Sources: {unique_sources}")
    if results:
        lines.append(f"  #1: {results[0].get('title','')[:100]}")

    intent_ok = intent == label or (
        label == "fresh" and intent in ("informational", "fresh")
    )
    mark = "✓" if intent_ok else "✗"
    lines.insert(0, f"  {mark}")
    return "\n".join(lines)


def main():
    all_results = []
    stats = {"total": 0, "correct": 0, "wrong": 0, "errors": 0, "total_latency": 0}

    print("=" * 80)
    print("INTENTFORGE V2 — STRESS TEST")
    print(f"{len(queries)} queries · {len(set(q for _, q in queries))} unique")
    print("=" * 80)

    for i, (qtype, query) in enumerate(queries):
        print(f"\n[{i+1}/{len(queries)}] ({qtype}) \"{query}\"")
        resp = search(query)
        all_results.append({"type": qtype, "query": query, "response": resp})

        if "error" in resp:
            print(f"  ✗ ERROR: {resp['error']}")
            stats["errors"] += 1
        else:
            analysis = analyze_result(qtype, qtype, query, resp)
            print(analysis)

            # Check correctness
            intent = resp.get("intent", "")
            expected = qtype
            if expected == "fresh":
                ok = intent in ("fresh", "informational")
            elif expected == "negative":
                ok = True  # just check it returns something
            elif expected == "multi-constraint":
                ok = True  # just check it returns something
            elif expected == "complex":
                ok = True  # just check it returns something
            else:
                ok = intent == expected

            if ok:
                stats["correct"] += 1
            else:
                stats["wrong"] += 1

            stats["total_latency"] += resp.get("_latency", 0)
        stats["total"] += 1

    # ─── Summary ───
    print("\n" + "=" * 80)
    print("SUMMARY")
    print("=" * 80)
    avg_latency = stats["total_latency"] / stats["total"] if stats["total"] else 0
    print(f"Total queries:    {stats['total']}")
    print(f"Correct intents:  {stats['correct']} ({stats['correct']/max(stats['total'],1)*100:.1f}%)")
    print(f"Wrong intents:    {stats['wrong']}")
    print(f"Errors:           {stats['errors']}")
    print(f"Average latency:  {avg_latency:.2f}s")

    # Print wrong intents
    if stats["wrong"] > 0:
        print(f"\nWrong intents:")
        for item in all_results:
            resp = item["response"]
            if "error" in resp:
                continue
            intent = resp.get("intent", "")
            expected = item["type"]
            if expected == "fresh" and intent in ("fresh", "informational"):
                continue
            if expected in ("negative", "multi-constraint", "complex"):
                continue
            if intent != expected:
                print(f"  \"{item['query']}\" → {intent} (expected {expected})")

    # Save full results
    with open(RESULTS_OUT, "w", encoding="utf-8") as f:
        json.dump(all_results, f, indent=2, default=str)
    print(f"\nFull results saved to {RESULTS_OUT}")


if __name__ == "__main__":
    main()
