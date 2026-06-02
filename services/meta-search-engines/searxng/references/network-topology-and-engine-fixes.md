# IntentForge Network Topology & Engine Fixes (v3 — 2026-06-01)

## FINAL STATE — All Engines

### SearXNG1 (VPN) — images/news/general
- General: bing, brave, startpage, wikipedia, mojeek, yandex, duckduckgo ✅
- Images: bing images ✅, openverse ✅, google images ❌ (VPN blocked), duckduckgo images ❌ (VPN blocked), brave images ⚠️ (timeout)
- News: bing news ✅, google news ✅
- Videos: bing videos ✅, duckduckgo videos ✅
- Tor/Onion: ahmia DISABLED (searx.network.tor integration broken — Python httpx SOCKS5 bug)

### SearXNG2 (Tor) — backup/fan-out
- General: bing, startpage, wikipedia, mojeek, yandex, duckduckgo ✅
- Images: bing images ✅, duckduckgo images ❌ (access denied), google images ❌ (access denied)
- Videos: bing videos ✅, duckduckgo videos ✅
- News: bing news ✅

### Invidious — YouTube video search
- Slow (12s+) but works. Gateway now has 3s timeout + SearXNG video fallback.

### Gateway /videos endpoint
- Now queries BOTH SearXNG (categories=videos) AND Invidious in parallel
- SearXNG: bing videos + duckduckgo videos (fast, reliable)
- Invidious: YouTube search (slow but unique content)
- Results merged and deduplicated by URL

## WHY ONLY BING IMAGES BEFORE

Root cause: `keep_only` whitelist in both settings.yml files only listed `bing images`.
No google images, duckduckgo images, brave images, or openverse were enabled.

### What was added:
- `google images` (engine: google_images) — **SUSPENDED: access denied** (VPN IP blocked by Google)
- `duckduckgo images` (engine: duckduckgo_extra, ddg_category: images) — **SUSPENDED: access denied** (VPN IP blocked by DDG)
- `brave images` (engine: brave, brave_category: images) — timeout (VPN congestion)
- `openverse` (engine: openverse) — **WORKING** (open API, no IP blocking)

### VPN IP blocking pattern:
Google and DuckDuckGo aggressively block known VPN/datacenter IPs for image search.
This is expected behavior — not a config issue. Workarounds:
1. Rotate VPN IPs (gluetun supports provider-specific rotation)
2. Use residential proxy
3. Accept bing images + openverse as reliable image sources

## AHMIA / TOR ENGINE FIX

### Problem:
`searx.network.tor` Python module has a bug with httpx SOCKS5 proxy support.
Error: `AttributeError: module 'httpx' has no attribute 'AsyncHTTPTransport'`
This causes ahmia to fail loading every 10s in a loop, spamming logs.

### Fix:
Disabled ahmia in settings.yml (`disabled: true`). Not critical — ahmia is dark web search.
The Tor SOCKS5 proxy itself works fine (tested with wget/curl). The issue is SearXNG's
Python httpx integration for SOCKS5.

### Tor optimization (preserved from earlier):
- NumEntryGuards=3 (was 1)
- EnforceDistinctSubnets=0
- ExitPolicy reject *:* (no exit)
- ClientPreferIPv6ORPort=0
- Clear stale state on restart

## QWANT IMAGES — NOT ADDED

Qwant requires a custom network definition (`network: qwant` in outgoing.networks).
The `keep_only` + `use_default_settings` mechanism doesn't pull in the qwant network
definition, causing `KeyError: 'qwant'` crash. Can be added later with explicit network config.

## VERIFIED RESULTS (post-fix)

| Endpoint | Hit Rate | Engines | Mean Latency | Results/Query |
|----------|----------|---------|--------------|---------------|
| /images  | 80% (4/5)| bing images, openverse | 1565ms | 44.5 avg |
| /videos  | 80% (4/5)| bing videos, duckduckgo videos, invidious | 2866ms | 59.3 avg |
| /news    | 100% (5/5)| google news, bing news | 1183ms | 83.8 avg |
| /search  | 100%     | bing, brave, startpage, mojeek, duckduckgo | 2500ms | 47 avg |
