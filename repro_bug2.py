import urllib.request, urllib.parse, json
q = "why is my internet speed much lower than the promised plan during peak hours"
url = "http://localhost:4000/search?q=" + urllib.parse.quote(q)
req = urllib.request.Request(url)
with urllib.request.urlopen(req, timeout=30) as resp:
    d = json.loads(resp.read())
print(f'intent={d["intent"]} conf={d["confidence"]:.2f} before={d["results_before_filter"]} after={d["results_after_filter"]} total={d["total"]}')
print(f'positive constraints: {d["structured_constraints"]["positive"]}')
print(f'negative constraints: {d["structured_constraints"]["negative"]}')
print(f'phrases: {d["structured_constraints"]["phrases"]}')
print(f'hard_exclusions: {d["structured_constraints"]["hard_exclusions"]}')
print(f'ignored: {d.get("ignored_constraints", "N/A")}')
