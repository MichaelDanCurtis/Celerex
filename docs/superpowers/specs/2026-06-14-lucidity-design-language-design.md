# Lucidity Design — Language & Skill Design Spec

**Date:** 2026-06-14
**Status:** Draft for review
**Scope:** Define the **Lucidity Design** language — the family-wide design language for the
Lucidity product family (OmniVyx Design, OmniVyx Agent, OmniVyx Vision, and any future
Galvo/Lucidity face) — and codify it as a reusable Claude skill (`lucidity-design`).
**Deliverables:** (1) the `lucidity-design` skill at `~/.claude/skills/lucidity-design/`;
(2) a one-page visual reference showcase (HTML) as the proof artifact.
**Synthesis of:** `nothing-design` (github.com/dominikmartn/nothing-design-skill) +
`liquid-glass` (github.com/haider-nawaz/liquid-glass-skill).
**Supersedes:** "OmniVyx Liquid Glass Studio" ([[omnivyx-design-language]]) — those tokens
become Lucidity Design's *Design product theme*. See [[omnivyx-architecture]].

---

## 1. Philosophy — the soul

**Lucidity is the moment of waking clarity** — the fuzziness that fades exactly as you
become fully awake. The whole language is that transition made structural:

> **Blur belongs to the periphery; clarity belongs to the center.**

- **Chrome is the haze.** The rail, toolbars, command surfaces, and the accent bloom are
  soft, translucent **Liquid Glass** — the dream still dissolving at the edges.
- **Content is the clarity.** Your data, your type, your hierarchy are **flat, monochrome,
  razor-sharp** (the Nothing discipline) — the fully-awake focus you arrive at.
- You move *through* fog (glass chrome) to land on *focus* (flat content). The name is the
  mechanic: **Lucidity = clarity emerging from soft fuzz.**

**Clarity is a spotlight (the focal plane).** The haze is not fixed — it is a *focus dial*.
To pull the eye, **reduce the glass on the thing that matters**: it resolves out of the fog
while everything around it stays soft. The UI has a **focal plane** like a camera's — what
is in focus is sharp, the rest is bokeh. This is the alarm-clock-in-the-morning effect: in a
blurry waking room, the single crisp, buzzing thing owns your attention. **Attention = the
inverse of blur.** Focus is therefore expressed two ways that can stack: **de-frosting**
(clarity) and the **accent** (color event) — the loudest "alarm" is the element that does
both.

Inherited principles (from Nothing): **subtract don't add; structure is ornament; monochrome
is the canvas, color is an event; type does the heavy lifting; industrial warmth.**

## 2. The Law (one inviolable rule)

**Content is flat; chrome is glass; never cross.** Everything you *read* is flat (Nothing).
Everything you *act through* is glass (Liquid Glass), floating over an accent bloom. The
**accent is the single color event** per screen. Both source systems already agree on this
boundary (Apple: "glass = navigation/chrome only, never content"; Nothing: "content is flat").

## 3. Decisions (locked in brainstorm 2026-06-14)

1. **Layer split** governs the fusion (§2).
2. **Nothing's discipline, Lucidity's own motif:** adopt Nothing's rigor (3-layer hierarchy,
   font budget, ALL-CAPS Space Mono labels, spacing-as-meaning, flat monochrome content) but
   **drop Doto / the dot-matrix** — the "one moment of surprise" is **our** geometry (the
   stacked-frame / squircle brand mark) + the glass bloom, not a Nothing homage.
3. **Per-product accent** plays Nothing's "red" interrupt role. Family default leans **red**
   (Nothing heritage + Design's red-lean); Agent = blue, Vision = purple.
4. **Web/CSS first** (the whole stack is React/Next/Vite); a SwiftUI `.glassEffect()` note
   for any future Apple target.
5. Skill name **`lucidity-design`**, lives at **`~/.claude/skills/lucidity-design/`**,
   triggers **explicitly only** ("Lucidity Design" / `/lucidity-design`) — never auto-fire.

## 4. Tokens — two families

### 4.1 Content tokens (Nothing-derived, flat)
- **Opacity hierarchy** (max 4 levels/screen): `--text-display` (100%) → `--text-primary`
  (~90%) → `--text-secondary` (~60%) → `--text-disabled` (~40%). The gray scale IS the
  hierarchy.
- **Type:** `--font-ui` Space Grotesk (UI/body), `--font-mono` Space Mono (labels/data,
  ALL CAPS, 0.06–0.1em tracking), hero numerals = Space Mono at display size. Type scale
  72→11px (Nothing's). **Two families, three sizes, two weights per screen** (the budget).
- **Spacing-as-meaning** (8px base, 2→96px): tight=belongs-together, vast=new-context.
  Dividers are a last resort.
- **Flat:** no shadow, no blur, no gradient on content. Light + dark both first-class
  (dark = OLED-ish near-black with a faint cool tint; light = warm-free off-white).

### 4.2 Chrome tokens (Liquid Glass, web/CSS — the translation)
The source glass skill is SwiftUI-only; Lucidity ships the **web/CSS** recipe:
`--glass-fill`, `--glass-border`, `--glass-specular` (brighter top edge), `--glass-blur`,
`--glass-saturate`, applied via `backdrop-filter: blur() saturate()` to **chrome only**, over
the **accent bloom** backdrop. `prefers-reduced-transparency` → solid fallback;
`prefers-reduced-motion` → kill bloom drift.

**Blur is a 3-stop focus dial** (the focal plane, §1) — glassiness encodes attention:
- `--glass-blur-deep` (~28px) — *recede*: backgrounded / dimmed / out-of-focus regions push
  further into haze, lower opacity.
- `--glass-blur` (~18px) — *base chrome*: the resting haze.
- `--glass-blur-clear` (~4px, → 0 for the sharpest focus) — *resolve*: the focal element
  de-frosts, with raised opacity + specular so it reads crisp.

State drives the dial: `hover/active/selected/urgent` step **toward clear**;
`idle/disabled/background` step **toward deep**. The focal element may also take the accent —
clarity + color = the loudest alarm. Transitions ride the `fog→focus` ease (§4.4): blur
animates down as an element gains focus, up as it loses it.

### 4.3 Accent
Per-product `--lucidity-accent`. The **one color event** — primary action, the single
interrupt, selected state — monochrome everywhere else. Data-status colors (success/warning)
exempt **only on the value**, never the label or row. Family default red; product themes
override (Agent blue, Vision purple).

### 4.4 Motion
Percussive, not fluid (Nothing): `--ease-out: cubic-bezier(0.25,0.1,0.25,1)`, 150–250ms
micro / 300–400ms transitions, **opacity over position** (elements fade, don't slide). One
signature motion expresses the philosophy: a brief **fog→focus** settle (a hair of blur
resolving to crisp) on first paint / context entry — used sparingly, never on every element.

## 5. Components, by layer
- **Glass (chrome):** rail, toolbars, command bar, floating controls, sheets, the bloom.
- **Flat (content):** cards, lists, tables, data viz (differentiate by opacity/pattern
  before color), hero numerals, prose. The lightest container that works (spacing → divider →
  border → surface, in that order); never box the most important element.
- **The one surprise per screen:** the stacked-frame/squircle mark + a glass bloom. Once.

## 6. The skill — `lucidity-design`

Structured like Nothing's proven layout, at `~/.claude/skills/lucidity-design/`:
- **`SKILL.md`** — philosophy (§1), the Law (§2), craft rules (3-layer, font budget,
  spacing-as-meaning, "one surprise"), merged **anti-patterns** (Nothing's *plus* glass-specific:
  never glass on content; glass needs a bloom to refract; no glass without the reduced-transparency
  fallback), and the workflow (declare fonts → pick mode → sketch the 3 layers → assign
  layer=content/chrome → compose → check tokens). Frontmatter: explicit trigger only.
- **`references/tokens.md`** — both token families (§4) with exact values + light/dark.
- **`references/components.md`** — components split by layer (§5), each with the web/CSS pattern.
- **`references/liquid-glass-web.md`** — the CSS glass recipe (blur+saturate, specular edge,
  bloom backdrop, fallbacks) — the web translation of the SwiftUI source.
- **`references/platform-mapping.md`** — HTML/CSS + React/Tailwind primary; SwiftUI
  `.glassEffect()` note for Apple targets.

## 7. Visual showcase (proof artifact)
A single self-contained `showcase.html` demonstrating the language live: a flat-content
panel beside glass chrome over an accent bloom, the type hierarchy, the accent-as-event, the
fog→focus motion, in light + dark. Lives with the skill; the SKILL.md references it. This is
how we "make it real" — something to *see*, not just read.

## 8. Relationship & rollout
- **Supersedes** OmniVyx Liquid Glass Studio as the canonical language. The OmniVyx tokens
  (`--ov-*`) become Lucidity Design's **Design product theme** (a themed instance).
- **Per-product application is a follow-up**, one product at a time (re-point each app's
  tokens to the Lucidity contract). Not in this spec.
- Galvo/substrate-shaped: the language is product-agnostic; each face supplies a theme
  (accent + bloom hue + light/dark default).

## 9. Out of scope
- Re-skinning the existing products onto Lucidity Design (follow-up per product).
- The native SwiftUI implementation (noted, not built — no Apple target yet).
- Reviving Doto/dot-matrix (deliberately dropped).

## 10. Locked visual realization (2026-06-14) — SKILL BUILT

The language was dialed in visually and **codified as the `lucidity-design` skill** at
`~/.claude/skills/lucidity-design/` (SKILL.md + references/{tokens, components,
liquid-glass-web, platform-mapping}.md + `showcase.html` — the canonical reference). The skill
files are now the **source of truth**; this spec is the design record.

Final decisions beyond the earlier sections:
- **Field = square-bokeh depth-of-field** (replaces the plain "bloom"): squares are pixels /
  *samples of reality* — small + crisp at the lucid focal center, large + soft into the dream
  periphery; **blur eases off toward the bottom** (grounded), **color goes glassier toward the
  top** (dream-glow rises); most squares are *faded shades of the background*, a few warm-beige
  / cool-glass squares **bloom through**; additive (screen) blend. (Earlier dither approach was
  rejected — too harsh; bokeh is the right read.) Generator in `liquid-glass-web.md` §3.
- **Atmosphere vs accent:** cool **teal** data-field + warm **beige** light bleed = the
  constant family signature; the warm **per-product accent** (default red-coral `#ff6a4d`;
  Agent `#38a8f0`, Vision `#9b6bf0`) is the single event. Decoupled.
- **Type:** Space Grotesk + Space Mono (ALL-CAPS instrument labels) + **Uncial Antiqua
  capitals** (chosen over script cursives — "otherworldly"), applied to display/brand caps via
  `unicode-range` (liquid-glass-web.md §5), never to mono labels. Corners **sharp** (3–4px),
  density **compact**.
- Resolved params: accent default `#ff6a4d`; fonts via Google Fonts CDN for v1, self-hosted
  woff2 required wherever the `unicode-range` uncial trick is used.

## 11. Out of scope / next
- Applying Lucidity Design to the actual products (re-skin OmniVyx Design + Agent onto the
  `lucidity-design` tokens, replacing the interim "OmniVyx Studio" `--ov-*` tokens) — a
  follow-up per product.
- Self-hosting the three fonts in each product for the unicode-range capitals.
