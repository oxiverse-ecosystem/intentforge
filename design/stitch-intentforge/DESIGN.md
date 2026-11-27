---
name: Terminal Precision
colors:
  surface: '#0f131c'
  surface-dim: '#0f131c'
  surface-bright: '#353942'
  surface-container-lowest: '#0a0e16'
  surface-container-low: '#181c24'
  surface-container: '#1c2028'
  surface-container-high: '#262a33'
  surface-container-highest: '#31353e'
  on-surface: '#dfe2ee'
  on-surface-variant: '#bacac2'
  inverse-surface: '#dfe2ee'
  inverse-on-surface: '#2c3039'
  outline: '#84948c'
  outline-variant: '#3b4a43'
  surface-tint: '#00e0af'
  primary: '#6fffd1'
  on-primary: '#00382a'
  primary-container: '#00e5b3'
  on-primary-container: '#00614a'
  inverse-primary: '#006c53'
  secondary: '#7bd0ff'
  on-secondary: '#00354a'
  secondary-container: '#00a6e0'
  on-secondary-container: '#00374d'
  tertiary: '#ffe2c3'
  on-tertiary: '#472a00'
  tertiary-container: '#ffbe6d'
  on-tertiary-container: '#784b00'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#42feca'
  primary-fixed-dim: '#00e0af'
  on-primary-fixed: '#002117'
  on-primary-fixed-variant: '#00513d'
  secondary-fixed: '#c4e7ff'
  secondary-fixed-dim: '#7bd0ff'
  on-secondary-fixed: '#001e2c'
  on-secondary-fixed-variant: '#004c69'
  tertiary-fixed: '#ffddb8'
  tertiary-fixed-dim: '#ffb95f'
  on-tertiary-fixed: '#2a1700'
  on-tertiary-fixed-variant: '#653e00'
  background: '#0f131c'
  on-background: '#dfe2ee'
  surface-variant: '#31353e'
typography:
  display-lg:
    fontFamily: Inter
    fontSize: 30px
    fontWeight: '600'
    lineHeight: 38px
    letterSpacing: -0.025em
  headline-lg:
    fontFamily: Inter
    fontSize: 22px
    fontWeight: '600'
    lineHeight: 28px
    letterSpacing: -0.02em
  headline-lg-mobile:
    fontFamily: Inter
    fontSize: 18px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: -0.015em
  headline-md:
    fontFamily: Inter
    fontSize: 16px
    fontWeight: '600'
    lineHeight: 22px
    letterSpacing: -0.01em
  headline-sm:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '600'
    lineHeight: 20px
    letterSpacing: -0.005em
  body-lg:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '400'
    lineHeight: 22px
    letterSpacing: 0em
  body-md:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 19px
    letterSpacing: 0em
  body-sm:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '400'
    lineHeight: 16px
    letterSpacing: 0.01em
  code-lg:
    fontFamily: JetBrains Mono
    fontSize: 13px
    fontWeight: '500'
    lineHeight: 20px
    letterSpacing: -0.01em
  code-md:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '400'
    lineHeight: 18px
    letterSpacing: -0.01em
  code-sm:
    fontFamily: JetBrains Mono
    fontSize: 11px
    fontWeight: '400'
    lineHeight: 15px
    letterSpacing: 0em
  label-md:
    fontFamily: JetBrains Mono
    fontSize: 11px
    fontWeight: '600'
    lineHeight: 14px
    letterSpacing: 0.06em
  label-sm:
    fontFamily: JetBrains Mono
    fontSize: 10px
    fontWeight: '500'
    lineHeight: 12px
    letterSpacing: 0.08em
  kbd:
    fontFamily: JetBrains Mono
    fontSize: 10px
    fontWeight: '600'
    lineHeight: 10px
    letterSpacing: 0.02em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 0.75rem
  gutter-md: 1rem
  margin: 1rem
  margin-md: 1.5rem
  space-2xs: 0.125rem
  space-xs: 0.25rem
  space-sm: 0.5rem
  space-md: 0.75rem
  space-lg: 1rem
  space-xl: 1.5rem
  space-2xl: 2rem
---

## Brand & Style

This design system delivers an environment of rigorous focus, computational transparency, and high cognitive ergonomics for technical researchers, intelligence analysts, and systems engineers. The aesthetic merges the unyielding density and clarity of an advanced developer console with the typographic and optical refinement of modern data-dense operating systems.

- **Audience:** Engineers, security researchers, and systems architects conducting deep intent discovery, cross-vector analysis, and local-versus-remote indexed exploration.
- **Atmosphere:** Calm, dark, terminal-adjacent, and strictly functional. The interface acts as an invisible lens over complex indices, surfacing provenance, latency metrics, and syntax execution states without visual noise.
- **Design Movement:** High-density utilitarian minimalism with structural border framing. The layout rejects floating decorative cards, rounded SaaS playfulness, pastel gradients, and illustrative marketing fluff. Visual hierarchy is established via semantic line weights, monospaced metadata anchoring, tight geometric alignment, and restrained tactical neon accents against ink-depth backdrops.

## Colors

The color architecture is built around absolute dark-canvas stability, relying on cold ink depths to maximize focus during extended query sessions while preserving contrast for fine micro-data.

- **Canvas & Structural Layers:**
  - Base Viewport Canvas: `#0B0F17` (Deep Obsidian Ink)
  - Surface Panel / Card Background: `#141B2D`
  - Elevated Popover / Context Menu Layer: `#1A233A`
  - Structural Borders & Grids: `#222F4C` (Crisp 1px baseline structural boundary)
- **Foreground & Text Contrast:**
  - Text Primary: `#F1F5F9` (Strict high-readability foreground for payload outputs, operators, and primary data)
  - Text Muted: `#94A3B8` (Descriptions, passive labels, parameter keys)
  - Text Subtle / Code Structural: `#64748B` (Brackets, structural dividers, inactive query operators)
- **Tactical & Functional Accents:**
  - Primary Action / Search Execution Accent: `#00E5B3` (Restrained tactical teal used exclusively for primary affirmative states, focused query inputs, and critical intent highlights)
  - Data Visualization & Graph Relations: `#38BDF8` (Cyan for telemetry curves and cross-reference links)
  - Warning / Memory Pressure: `#F59E0B` (Amber for uncommitted filters and cold caches)
- **Provenance & Telemetry Badges:**
  - Local Vector Index: Foreground `#10B981`, surface tint `rgba(16, 185, 129, 0.12)`, border `#047857`
  - Web Index Node: Foreground `#38BDF8`, surface tint `rgba(14, 165, 233, 0.12)`, border `#0EA5E9`
  - Cached Engine State: Foreground `#A78BFA`, surface tint `rgba(139, 92, 246, 0.12)`, border `#8B5CF6`
  - Hardware Execution / Alert: Foreground `#EF4444`, surface tint `rgba(239, 68, 68, 0.12)`, border `#991B1B`

## Typography

The typography system relies on a two-tier structural pairing:
- **Inter** provides neutral, optical balance for system navigation, operational labels, and continuous reading of intent outputs.
- **JetBrains Mono** powers operational telemetry, latency indicators, JSON inspection payloads, regex match highlights, provenance markers, and keyboard affordances.

### Application Rules
1. **Case & Tracking:** Labels smaller than 12px using the monospace token must be rendered in uppercase with positive letter spacing (`0.06em` to `0.08em`) to guarantee quick recognition on dense dashboards.
2. **Numeric Alignment:** All monospaced tokens apply `tabular-nums` by default, ensuring layout stability during real-time indexing, byte count tracking, and microsecond telemetry ticks.
3. **No Stylistic Distractions:** Keep weight gradations tight (predominantly 400 for structural text, 500/600 for active states and identifiers). Never exceed 600 weight.

## Layout & Spacing

The layout model favors dense modular utility over decorative white space. The viewport is divided using a fixed structural pane framework (Command Rail, Query & Filter Inspector, Stream View, Detail/Trace Inspector).

### Grid & Density Hierarchy
- **Desktop (1280px+):** Collapsible 3-column architecture (Left Nav/History Rail 240px, Main Stream Fluid min 640px, Inspector/JSON Tree 380px fixed). Gutters between docking frames use `0.75rem` or flush `1px` structural borders.
- **Tablet (768px - 1279px):** Split layout with the Inspector offloaded to a slide-over sheet or secondary tab pane. Gutter defaults to `0.75rem`.
- **Mobile (< 768px):** Single-column stacked mode. Query input docks to top, persistent status/shortcut drawer sits anchored to bottom. Margins compress to `0.75rem`.

### Spacing Rhythm
Spacing tokens conform strictly to an 8px / 4px incremental scale, with micro-tokens (`0.125rem` / `0.25rem`) used for metadata tag padding, key-value alignment, and shortcut badge insets.

## Elevation & Depth

This design system avoids soft, atmospheric drop-shadows and blurred multi-colored glows. Depth is communicated strictly through surface brightness tiers, crisp boundary lines, and structural opacity stepping.

- **Level 0 (Canvas Base):** `#0B0F17` — Primary terminal workspace and layout background.
- **Level 1 (Docked Panels & Data Tables):** `#141B2D` framed by a 1px solid border of `#222F4C`.
- **Level 2 (Active Focus & Card Hover):** `#1A233A` with `#00E5B3` 1px border accent or active corner tick.
- **Level 3 (Modal Shells & Command Palettes):** `#141B2D` background paired with a razor-thin border of `#38BDF8` (or `#222F4C`) and a controlled, dark falloff shadow: `0 12px 32px -4px rgba(2, 6, 23, 0.85)`.
- **Ghost Outlines:** Inactive inputs, passive chips, and unselected facets use a 1px border (`#222F4C`) over a transparent or Level 1 background. No blur filters are permitted on content cards.

## Shapes

The shape system is tightly constrained (`roundedness: 1` — base `0.25rem` / 4px radius) to project engineering precision and maximize usable screen area. 

- **Containers & Panels:** Fixed 4px corner radius (`rounded`) or completely flush 0px within bordered multi-pane views.
- **Inputs & Buttons:** 4px radius (`rounded`).
- **Keyboard Badges & Status Tags:** 2px to 4px radius (`rounded-sm`).
- **Pill Shapes (Forbidden):** Full rounded pill shapes (`9999px`) are strictly prohibited to avoid playful, consumer-app connotations. Everything adheres to clean rectangular geometry.

## Components

### 1. Command Bar & Search Input
- **Structure:** Full-width terminal query box with fixed left icon/prompt indicator (`>_`), interactive token highlighting, right-aligned query latency readout (`1.42ms`), and shortcut indicator (`⌘K` or `/`).
- **States:** Inactive border `#222F4C`; Focused border `#00E5B3` with a zero-offset hard focus ring (`outline: 1px solid #00E5B3`).
- **Typography:** `code-lg` with real-time semantic highlighting (operators in `#00E5B3`, syntax parameters in `#38BDF8`, string values in `#F1F5F9`).

### 2. Buttons & Actions
- **Primary:** Background `#00E5B3`, text `#0B0F17`, font `JetBrains Mono` 12px 600 weight. No drop shadow. Hover state: brightness 108%. Active state: scale 0.99.
- **Secondary / Ghost:** Background `transparent`, border 1px solid `#222F4C`, text `#F1F5F9`. Hover: background `#1A233A`, border-color `#38BDF8`.
- **Danger:** Background `transparent`, border 1px solid `#991B1B`, text `#EF4444`. Hover: background `rgba(239, 68, 68, 0.1)`.

### 3. Keyboard Shortcut Indicators (`<kbd>`)
- **Structure:** Inline micro-capsule. Height 18px, padding horizontal 4px.
- **Styling:** Background `#141B2D`, border 1px solid `#222F4C`, text `#94A3B8`, typography `kbd` (`JetBrains Mono`, 10px). Placed systematically inside search inputs, dropdown action rows, and panel tabs.

### 4. Chips & Index Badges
- **Index Origin Badges:** Flat backgrounds with 1px border. Height 20px, font `label-sm`.
  - *Local Vector:* Background `rgba(16, 185, 129, 0.1)`, border `#047857`, text `#10B981`. Includes a pulsing 4px emerald indicator dot.
  - *Remote Web:* Background `rgba(14, 165, 233, 0.1)`, border `#0EA5E9`, text `#38BDF8`.
  - *Cache State:* Background `rgba(139, 92, 246, 0.1)`, border `#8B5CF6`, text `#A78BFA`.
- **Filter Facets:** Inactive border `#222F4C`, text `#94A3B8`. Active border `#00E5B3`, text `#00E5B3`, background `rgba(0, 229, 179, 0.05)`.

### 5. Data Density Cards & Result Rows
- **Layout:** Horizontal compact rows or flush-bordered cards. Padding `space-md` (`0.75rem`).
- **Header:** Title in `headline-sm` with trailing vector score badge (e.g. `0.9842` in `code-sm` / text `#38BDF8`).
- **Body:** Matched content snippet with target terms highlighted in bold `#00E5B3` on an ambient background tint (`rgba(0, 229, 179, 0.1)`).
- **Footer:** Metadata trail: provenance index icon, size metrics, crawl/embed timestamp in `code-sm` (`#64748B`), and quick-action hotkeys (e.g., `[Y] Copy Raw JSON`, `[D] Diff`).

### 6. Inputs, Checkboxes & Radio Controls
- **Inputs:** Background `#0E131F`, border 1px solid `#222F4C`, text `#F1F5F9`. Placeholder `#64748B`.
- **Checkboxes:** 14px x 14px square, radius 2px. Border `#222F4C`. Checked: background `#00E5B3`, border `#00E5B3`, check mark icon `#0B0F17`.
- **Radio Buttons:** 14px circle, border `#222F4C`. Selected: border `#00E5B3`, inner solid teal core (6px).

### 7. JSON Trace & Raw Payload Viewers
- **Structure:** Monospaced inspector pane with line numbering (`#64748B`), syntax highlighting, and expandable fold arrows (`▾`, `▸`).
- **Background:** `#0B0F17` inset within `#141B2D` frame. Border 1px solid `#222F4C`.