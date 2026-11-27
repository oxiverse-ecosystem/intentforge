# IntentForge UI — Stitch Prototype

Static, high-fidelity UI prototype generated with [Google Stitch](https://stitch.google.com/) from a brief covering every IntentForge API surface.

- **Stitch project:** https://stitch.google.com/projects/15795196413537648124
- **Exported:** 2026-11-27 via Stitch → Export → `.zip`
- **Design tokens:** see [DESIGN.md](DESIGN.md) (`Terminal Precision` palette)

## Files

| File | Screen |
|---|---|
| `index.html` | Navigator shell — switch screens, desktop/mobile viewport, keyboard `1–5` |
| `01-search.html` | Search + right-hand reasoning inspector (intent, spelling, negation, geo, `/inspect`) |
| `02-media.html` | Images / Videos / News / Shopping verticals |
| `03-goals.html` | Goal creation, Q&A, phased roadmap, leaderboard |
| `04-api.html` | Developer API console and operator reference |
| `05-settings.html` | Backend, Cloudflare Tunnel, cache, rate limits, theme |
| `screens/*.png` | Stitch's original screen previews |

Open `index.html` directly in a browser; no build step or server is required.

## Scope and caveats

- These are **design prototypes**, not production frontend code. Form submissions and buttons are not wired to the gateway.
- Sample data is illustrative. Counts, statuses and timings shown in the mockups are not live measurements.
- The generated HTML loads Inter, JetBrains Mono and Material Symbols from Google Fonts, so an internet connection is needed for exact typography/icons.
- Stitch reported transient `400 / 403 / 429` messages while generating because its daily credit/rate limits were hit, but it completed all five screens and the design-system document.

## Regenerating

Edit the prompt in the Stitch project above and export again with **Export → .zip**. Replace the `0*.html` files and `DESIGN.md`; keep `index.html` if the screen filenames stay the same.

To iterate on the visual language without Stitch, change the CSS variables in `index.html` and the `:root` tokens inside each screen's HTML.
