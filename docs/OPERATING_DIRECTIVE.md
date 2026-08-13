# IntentForge Operating Directive (2026-08-13)

NOTE: the repo's `.gitignore` intentionally ignores root `ROADMAP.md` (local/throwaway by convention).
This tracked file is the authoritative operating directive the set-and-forget loop reads.

## 🔒 Hard constraints (non-negotiable)

1. **FEATURE-FROZEN.** No new user-facing features are added to IntentForge. The architecture
   (VPN+Tor routing, BERT intent classification, privacy-by-design gateway) is complete. The loop must
   NOT propose or implement new capabilities. Scope is closed.

2. **LATENCY IS THE ONLY OPEN PROBLEM — AND TIMEOUTS ARE NOT THE SOLUTION.**
   Do NOT "solve" latency by tightening agent/think timeouts. A timeout that cuts off reasoning trades
   quality for speed and produces worse results. That is a fake fix.
   Real latency work optimizes the CONNECTION/PATH layer, progressively, while preserving quality:
   - Tor circuit selection / pre-building stable guards (avoid slow ephemeral circuits)
   - VPN endpoint proximity + keep-alive (reduce handshake/relay hops)
   - SearXNG/proxy choice + warm pools; connection reuse; DNS prefetch
   - progressive response: stream/return partial results as they finalize so the user sees progress
     immediately, while the full answer still completes with full quality
   - measure tail latency (p95/p99), not averages.

3. **QUALITY AND SPEED TOGETHER.** Every latency change must be verified to NOT regress answer quality.
   Progressive delivery is the mechanism: fast first token + complete correct answer.

## 🎯 Dynamic, phase-driven roadmap

Tasks are generated PER PHASE. After a phase completes, the NEXT phase's tasks are derived from THAT phase's
outcome — relevant to the phase just finished, not speculative new features. No backlog of unrelated extras.

### Phase A (CURRENT): Connection-layer latency hardening
- Tor guard pre-selection + circuit pinning (stable, low-latency guards)
- VPN endpoint proximity map + keep-alive health ping
- SearXNG instance warm-pool + failover ranked by measured latency
- Progressive/streaming response path gateway → user (first result fast, full result complete)
- Latency telemetry: p50/p95/p99 per hop (Tor, VPN, SearXNG, classify)

### Phase B (derived from A): Consolidate + regression-guard
- Lock latency gains with a CI latency budget (fail if p95 regresses beyond threshold)
- Quality regression test: sampled queries must keep answer correctness vs baseline
- (Generated from Phase A's measured bottlenecks — NOT new features.)

### Phase C+ : only what Phase B's data demands
- Tasks produced from prior-phase evidence. No feature addition. If connection layer is saturated,
  the work is deeper connection tuning — not new product surface.

## 🚫 Out of scope (do not build)
New search modes, new UIs, new storage backends, new ML models, new endpoints beyond what exists.
If the loop is tempted to "add" something, the answer is NO — optimize the path, do not expand the surface.
