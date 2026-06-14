# OmniVyx Design Language — Design Spec

**Date:** 2026-06-13
**Status:** Draft for review
**Scope:** Spec #1 of 2. This spec defines the *shared visual design language* for the
two OmniVyx products. The *product re-architecture* (Agent = action system, Rituals
rename, connector/tool + document-workspace migrations) is **spec #2** and is out of
scope here — it builds on this one.
**Applies to both repos:** `Celerex` (OmniVyx Agent) and `celerex-design` (OmniVyx Design).

---

## 1. Problem & goal

Today the two products read as **two different products**, not siblings:

- **OmniVyx Design** has a mature design system (full token sheet, light/dark, radius/
  shadow/motion scales, CSS Modules + a `packages/components` primitives layer) — but it
  "just looks like Anthropic": warm-beige canvas (`#faf9f7`), Source Serif headings, soft
  pills everywhere.
- **OmniVyx Agent** is an engineering UI: ~16 `--celerex-*` variables and otherwise inline
  `React.CSSProperties` everywhere, system fonts, blue accent, no component layer.

**Goal:** one shared design language so the two apps read as **siblings of one suite** —
*identical system, distinct accent* (decided: Option A). Not pixel-identical; the accent
hue is the deliberate per-product differentiator. The language must be visually
**distinctive and explicitly not "Anthropic"**, rooted in OmniVyx's own brand DNA (dark
squircle, stacked-frame motif, geometric icon family, per-product hues).

## 2. Locked decisions (from brainstorm)

1. **Ideology = "Studio + Liquid Glass."** Expressive, accent-as-surface, brand geometry
   (the stacked-frame motif becomes UI), on a translucent **Liquid Glass** material
   (frosted, blurred, saturated, with specular light edges) floating over an
   accent-bloomed backdrop.
2. **Per-product accent** (same system, swapped hue + bloom):
   - **OmniVyx Design** → red-leaning. **Magenta `#f25c8a` is the working placeholder;**
     the intended accent trends toward a deeper, almost-crimson red. Exact value TBD —
     it is a single token, swappable with zero structural change.
   - **OmniVyx Agent** → blue `#38a8f0`.
3. **Typography** = **geometric display + clean body.** An expressive geometric/display
   face for large titles + hero numerals; a neutral modern sans for body. **Source Serif
   is removed** (it is the primary "Anthropic" tell).
4. **Glass is the signature, not the whole wardrobe** — see material tiers (§3).
5. **Sharing mechanism** = vendored token contract, per-app components (§5).

## 3. Material tiers

Three tiers. Glass is reserved for chrome / hero / cards; dense text never sits on live
glass (legibility + perf).

| Tier | Surface | Where it's used |
|---|---|---|
| **Glass-dark "Studio"** | Frosted translucent panels over a dark, accent-bloomed backdrop; specular top edge. | Creating moments, hero/empty states, dashboards, the brand's first impression. |
| **Glass-light** | Frost over a pale accent-tint backdrop. | Mixed work + reading where dark is too heavy. |
| **Solid-dense "Command"** | No glass. Hairline rows, mono metadata, square edges, monochrome + one accent. | High-density lists/tables (e.g. 175 agents, ritual logs) where glass wrecks legibility. |

**Rule:** a view picks exactly one tier for its primary surface. Glass tiers may *contain*
solid-dense regions (e.g. a glass card wrapping a hairline table is fine — the table itself
is solid). Never stack live glass behind scrolling body text.

## 4. Token contract — `omnivyx-tokens.css`

One canonical token sheet, vendored into both apps (§5). Starting values below; the neutral
ramp is **cool/neutral, never warm-beige**. Light + dark. Per-product files override only
`--ov-accent*` and the bloom hues.

### 4.1 Neutrals (cool) — dark (Studio default)
```
--ov-bg:            #191320   /* studio base (accent-tinted near-black) */
--ov-bg-deep:       #120e18
--ov-surface:       #201a28   /* solid panels (Command tier) */
--ov-text:          #f2eef6
--ov-text-muted:    rgba(255,255,255,0.66)
--ov-text-faint:    rgba(255,255,255,0.40)
--ov-hairline:      rgba(255,255,255,0.10)
```

### 4.2 Neutrals (cool) — light
```
--ov-bg:            #f7f7fb   /* cool, NOT #faf9f7 beige */
--ov-surface:       #ffffff
--ov-text:          #14151a
--ov-text-muted:    #6b7280
--ov-hairline:      #e6e6ec
```

### 4.3 Glass material tokens
```
--ov-glass-fill:        rgba(255,255,255,0.10)   /* light tint on dark; mirror for light mode */
--ov-glass-fill-strong: rgba(255,255,255,0.16)
--ov-glass-border:      rgba(255,255,255,0.16)
--ov-glass-specular:    rgba(255,255,255,0.40)   /* brighter top edge */
--ov-glass-blur:        22px
--ov-glass-saturate:    1.6
--ov-glass-tint:        color-mix(in srgb, var(--ov-accent) 22%, transparent) /* accent glass */
```

### 4.4 Accent (per product — the ONE differentiator)
```
--ov-accent:        #f25c8a   /* Design: placeholder magenta → red-leaning TBD; Agent: #38a8f0 */
--ov-accent-bloom:  #f25c8a   /* dominant backdrop bloom */
--ov-accent-bloom2: #38a8f0   /* secondary family-hint bloom (the sibling's hue) */
--ov-accent-strong: color-mix(in srgb, var(--ov-accent) 86%, var(--ov-text))
--ov-accent-soft:   color-mix(in srgb, var(--ov-accent) 22%, var(--ov-surface))
--ov-accent-tint:   color-mix(in srgb, var(--ov-accent) 12%, var(--ov-surface))
```

### 4.5 Shape, motion, type
```
--ov-radius-xs: 6px;  --ov-radius: 12px;  --ov-radius-lg: 16px;  --ov-radius-pill: 999px;
--ov-ease:      cubic-bezier(0.23, 1, 0.32, 1);
--ov-dur-enter: 200ms;  --ov-dur-exit: 140ms;  --ov-dur-quick: 120ms;
--ov-font-display: <geometric display face>, system-ui, sans-serif;   /* large titles/numerals */
--ov-font-sans:    <clean modern sans>, -apple-system, system-ui, sans-serif; /* body */
--ov-font-mono:    ui-monospace, "SF Mono", Menlo, monospace;          /* Command metadata */
```
(Exact display/body families chosen during implementation; must be self-hostable, not
Source Serif.)

## 5. Sharing across two no-shared-runtime-code repos

The two products **share no runtime code by design** (only a bundle format + HTTP seam).
The design language follows the same philosophy as a **contract, not a library**:

- **`omnivyx-tokens.css`** (the §4 sheet) is **vendored** (copied) into each app, plus a
  small per-product override file setting `--ov-accent*` + bloom hues. This mirrors how the
  CDB bundle format is a copied contract, not shared code.
- **Each app implements its own glass components** (TopBar, GlassCard, GlassButton,
  CommandRow, Bloom backdrop) from the shared tokens. No shared component npm package to
  publish/version.
- Drift control: the token file carries a `/* contract version: N */` header; a bump is a
  deliberate, reviewed copy into both apps.

Rejected alternatives: a published `@omnivyx/design-tokens` package (adds publish/version
friction for a 2-consumer system) and a full shared component library (violates the
no-shared-code principle, heavy).

## 6. Liquid Glass — web implementation rules

(The iOS `glassEffect()` API does not apply; these are the web equivalents.)

- **Material:** `background: var(--ov-glass-fill)` + `backdrop-filter: blur(var(--ov-glass-blur))
  saturate(var(--ov-glass-saturate))` (+ `-webkit-` prefix).
- **Specular edge:** 1px border `var(--ov-glass-border)` with `border-top-color:
  var(--ov-glass-specular)` for the light-catching top edge.
- **Never opaque behind glass** — glass must sit over the bloomed backdrop (or other
  content) or the effect dies. The Bloom backdrop (radial accent + family-hint blooms over
  `--ov-bg-deep`) is what glass refracts.
- **Tint for prominence:** primary actions use accent glass (`--ov-glass-tint`), not solid
  accent, except where contrast demands solid.
- **Contrast on glass:** body/labels on glass must hold AA contrast against the *darkest*
  region of the backdrop behind them; prefer `--ov-text` / `--ov-text-muted`, never raw
  gray.
- **Performance:** limit simultaneous live-glass layers per view (≈ ≤6 large surfaces);
  do not nest glass inside glass; dense regions use the solid tier.

## 7. Accessibility & fallbacks

- **`prefers-reduced-transparency`** → glass tiers fall back to the **solid tier** (opaque
  `--ov-surface`, keep border + radius, drop blur/translucency).
- **`prefers-reduced-motion`** → disable bloom drift / morph transitions; keep instant
  state changes.
- All accent-on-surface and text-on-glass pairings verified for WCAG AA. The red-leaning
  Design accent's final value must pass AA for button text (white-on-accent) before lock.

## 8. Rollout sequencing

1. **Author `omnivyx-tokens.css`** (+ the two per-product override files) and the glass
   component set spec.
2. **Re-skin OmniVyx Agent first** — it has the larger gap (inline styles → tokens) and is
   the repo we're in. Introduce a thin primitives layer (GlassCard/Button/TopBar/CommandRow
   + Bloom) and migrate shell → nav → Settings → dashboards first; dense views adopt the
   solid Command tier. This *also* retires the ad-hoc inline-style debt.
3. **Adopt in OmniVyx Design** — re-namespace its existing tokens to `--ov-*`, drop Source
   Serif for the geometric display/body pair, apply glass tiers to hero/creation surfaces,
   keep its dense views on the solid tier. (Design already has the system rigor; this is
   mostly re-hue + re-type + glass adoption, not a rebuild.)

Each step is its own implementation plan (writing-plans), branched + PR'd.

## 9. Out of scope (→ spec #2 or later)

- Product re-architecture: Agent = action system; **Orbit → Ritual** rename; connectors/
  tools management → Agent; document-workspace concept → Design. (Spec #2.)
- The exact red accent value for Design (parameter; magenta placeholder until chosen).
- Final display/body typeface selection (chosen at implementation; personality = geometric
  display + clean body is locked).
- A component-by-component rewrite of every existing screen — rollout is incremental per §8.

## 10. Open parameters

- **Design accent:** exact red-leaning value (AA-verified) — TBD.
- **Typefaces:** specific self-hostable display + body families — TBD at implementation.
- **Glass-light tier:** exact light-mode glass alphas — tuned during Agent re-skin.
