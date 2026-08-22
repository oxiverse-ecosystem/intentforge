import json, urllib.request

B = "http://localhost:4000"

def post(path, payload):
    req = urllib.request.Request(
        B + path,
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read().decode())

def check(label, d):
    rm = d.get("roadmap", {})
    tp = rm.get("total_phases")
    lp = len(rm.get("phases", []))
    tl = d.get("total_phases")
    ok = tp == lp
    print(f"[{label}] roadmap.total_phases={tp} len(phases)={lp} top-level total_phases={tl} INVARIANT_OK={ok}")
    return ok

print("=== QUICK FLOW ===")
q = post("/goals/quick", {"goal": "learn rust programming in 3 months"})
ok1 = check("quick", q)

print("=== ANSWERS FLOW ===")
g = post("/goals", {"goal": "write a novel in 6 months"})
gid = g["goal_id"]
print("created goal_id =", gid)
a = post(f"/goals/{gid}/answers", {"answers": [
    {"question_id": 1, "answer": "6 months"},
    {"question_id": 2, "answer": "5-10 hours"},
]})
ok2 = check("answers", a)

print("\nALL PASS =", ok1 and ok2)
