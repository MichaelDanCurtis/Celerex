# OmniVyx Agent Re-Skin (Design Language Increment 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish the OmniVyx design-language token contract + Liquid Glass primitives in the Agent frontend and re-skin the app shell (background bloom, top bar, sidebar) so OmniVyx Agent reads as the sibling of OmniVyx Design.

**Architecture:** Add a product-agnostic `omnivyx-tokens.css` contract (`--ov-*`) + an Agent accent/bloom theme file, then **re-point the app's existing `--celerex-*` variables to the new `--ov-*` tokens** so the entire color layer re-skins through one indirection (no rewrite of the ~700 inline styles). Add a small set of glass primitive components + a `BloomBackdrop`, and apply them to the shell chrome only (the dense content views stay on the calm/solid tier per the spec's material-tier rule). Visual tasks are verified by `preview` screenshots, not jsdom unit tests (jsdom does not apply CSS); component tasks get vitest render tests.

**Tech Stack:** React + jotai + Vite, vitest + @testing-library/react + jsdom, plain CSS custom properties, CSS `backdrop-filter`. Preview via the `celerex-frontend` launch config on port 5173.

**Spec:** `docs/superpowers/specs/2026-06-13-omnivyx-design-language-design.md`

---

## File Structure

- Create: `packages/frontend/app/src/styles/omnivyx-tokens.css` — the `--ov-*` token contract (light + dark).
- Create: `packages/frontend/app/src/styles/omnivyx-theme-agent.css` — Agent's accent (`#38a8f0`) + bloom hues.
- Create: `packages/frontend/app/src/styles/omnivyx.css` — glass utility classes (`.ov-glass`, `.ov-glass-bar`, `.ov-glass-rail`, `.ov-bloom*`).
- Create: `packages/frontend/app/src/components/omnivyx/BloomBackdrop.tsx` — the accent-bloom field glass refracts.
- Create: `packages/frontend/app/src/components/omnivyx/GlassButton.tsx`, `GlassPanel.tsx` — primitives.
- Create: `packages/frontend/app/src/components/omnivyx/__tests__/glass.test.tsx` — render tests.
- Modify: `packages/frontend/app/src/main.tsx` — import the new CSS (3 lines).
- Modify: `packages/frontend/app/index.html` — load display + body webfonts.
- Modify: `packages/frontend/app/src/styles/global.css` — re-point `--celerex-*` → `--ov-*`; body font.
- Modify: `packages/frontend/app/src/App.tsx` — mount `BloomBackdrop` in shell; glass the Toolbar root.
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx` — glass the rail.

---

## Task 1: Token contract + theme + fonts

**Files:**
- Create: `packages/frontend/app/src/styles/omnivyx-tokens.css`
- Create: `packages/frontend/app/src/styles/omnivyx-theme-agent.css`
- Modify: `packages/frontend/app/index.html`
- Modify: `packages/frontend/app/src/main.tsx`

- [ ] **Step 1: Create `omnivyx-tokens.css`**

```css
/* OmniVyx design-language token contract — vendored into both products. */
/* contract version: 1 */
:root {
  --ov-bg: #f7f7fb;
  --ov-bg-deep: #ecebf2;
  --ov-surface: #ffffff;
  --ov-surface-2: #f4f4f8;
  --ov-text: #14151a;
  --ov-text-muted: #6b7280;
  --ov-text-faint: #9aa0ab;
  --ov-hairline: #e6e6ec;

  --ov-glass-fill: rgba(255, 255, 255, 0.55);
  --ov-glass-fill-strong: rgba(255, 255, 255, 0.72);
  --ov-glass-border: rgba(255, 255, 255, 0.65);
  --ov-glass-specular: rgba(255, 255, 255, 0.92);
  --ov-glass-blur: 20px;
  --ov-glass-saturate: 1.6;

  --ov-accent: var(--ov-accent-base, #38a8f0);
  --ov-accent-strong: color-mix(in srgb, var(--ov-accent) 86%, var(--ov-text));
  --ov-accent-soft: color-mix(in srgb, var(--ov-accent) 22%, var(--ov-surface));
  --ov-accent-tint: color-mix(in srgb, var(--ov-accent) 12%, var(--ov-surface));
  --ov-glass-tint: color-mix(in srgb, var(--ov-accent) 24%, transparent);

  --ov-radius-xs: 6px;
  --ov-radius: 12px;
  --ov-radius-lg: 16px;
  --ov-radius-pill: 999px;

  --ov-ease: cubic-bezier(0.23, 1, 0.32, 1);
  --ov-dur-enter: 200ms;
  --ov-dur-exit: 140ms;
  --ov-dur-quick: 120ms;

  --ov-font-display: "Space Grotesk", system-ui, sans-serif;
  --ov-font-sans: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif;
  --ov-font-mono: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
}

:root[data-theme="dark"] {
  --ov-bg: #191320;
  --ov-bg-deep: #120e18;
  --ov-surface: #201a28;
  --ov-surface-2: #271f30;
  --ov-text: #f2eef6;
  --ov-text-muted: rgba(255, 255, 255, 0.66);
  --ov-text-faint: rgba(255, 255, 255, 0.40);
  --ov-hairline: rgba(255, 255, 255, 0.10);

  --ov-glass-fill: rgba(255, 255, 255, 0.10);
  --ov-glass-fill-strong: rgba(255, 255, 255, 0.16);
  --ov-glass-border: rgba(255, 255, 255, 0.16);
  --ov-glass-specular: rgba(255, 255, 255, 0.40);
}

@media (prefers-reduced-transparency: reduce) {
  :root {
    --ov-glass-fill: var(--ov-surface);
    --ov-glass-fill-strong: var(--ov-surface);
    --ov-glass-blur: 0px;
  }
}
```

- [ ] **Step 2: Create `omnivyx-theme-agent.css`** (Agent = blue; secondary bloom = the Design sibling's magenta as a family hint)

```css
/* OmniVyx Agent product theme — the only per-product overrides. */
:root {
  --ov-accent-base: #38a8f0;
  --ov-bloom-1: #38a8f0;
  --ov-bloom-2: #f25c8a;
}
```

- [ ] **Step 3: Load the webfonts** — in `packages/frontend/app/index.html`, inside `<head>` (before the existing inline `<style>`), add:

```html
<link rel="preconnect" href="https://fonts.googleapis.com" />
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
<link
  href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600&family=Inter:wght@400;500;600&display=swap"
  rel="stylesheet"
/>
```

(Self-hosting these is a follow-up; CDN is fine for now per the spec's open-parameters note.)

- [ ] **Step 4: Import the token CSS** — in `packages/frontend/app/src/main.tsx`, immediately after the existing `import "./styles/global.css";` line, add:

```ts
import "./styles/omnivyx-tokens.css";
import "./styles/omnivyx-theme-agent.css";
```

- [ ] **Step 5: Verify the app still boots**

Run: `cd packages/frontend/app && bunx vite build 2>&1 | tail -5`
Expected: build completes with no CSS parse errors.

Then start preview and screenshot:
- `preview_start` name `celerex-frontend`
- `preview_console_logs` level `error` → expect none new
- `preview_screenshot` → app renders unchanged (tokens defined but not yet applied).

- [ ] **Step 6: Commit**

```bash
git add packages/frontend/app/src/styles/omnivyx-tokens.css \
        packages/frontend/app/src/styles/omnivyx-theme-agent.css \
        packages/frontend/app/index.html packages/frontend/app/src/main.tsx
git commit -m "feat(design): add OmniVyx token contract + Agent theme + fonts"
```

---

## Task 2: Re-point `--celerex-*` to `--ov-*` (re-skin the color layer app-wide)

**Files:**
- Modify: `packages/frontend/app/src/styles/global.css:1-23,36-42`

- [ ] **Step 1: Replace the `:root` celerex block** (lines 1–12) with var references:

```css
:root {
  --celerex-bg-primary: var(--ov-bg);
  --celerex-bg-secondary: var(--ov-surface-2);
  --celerex-bg-sidebar: var(--ov-surface);
  --celerex-text-primary: var(--ov-text);
  --celerex-text-secondary: var(--ov-text-muted);
  --celerex-border: var(--ov-hairline);
  --celerex-accent: var(--ov-accent);
  --celerex-accent-hover: var(--ov-accent-strong);
  --celerex-sidebar-width: 260px;
  --celerex-ai-panel-width: 340px;
}
```

- [ ] **Step 2: Replace the `:root[data-theme="dark"]` celerex block** (lines 14–23) with the *same* var references (the `--ov-*` values already switch under dark, so the mapping is identical):

```css
:root[data-theme="dark"] {
  --celerex-bg-primary: var(--ov-bg);
  --celerex-bg-secondary: var(--ov-surface-2);
  --celerex-bg-sidebar: var(--ov-surface);
  --celerex-text-primary: var(--ov-text);
  --celerex-text-secondary: var(--ov-text-muted);
  --celerex-border: var(--ov-hairline);
  --celerex-accent: var(--ov-accent);
  --celerex-accent-hover: var(--ov-accent-strong);
}
```

- [ ] **Step 3: Set the body font** — in `global.css` `body { ... }` (line ~37) replace the `font-family` value with:

```css
  font-family: var(--ov-font-sans);
```

- [ ] **Step 4: Verify**

- Reload preview (`preview_eval` → `window.location.reload()`), `preview_console_logs` level `error` (expect none).
- `preview_screenshot` → accents are now OmniVyx blue `#38a8f0` (the "AI Panel"/active items), neutrals shifted cool, body font is Inter. Layout intact.
- Toggle dark via the Toolbar and screenshot once → dark neutrals come from `--ov-*` dark values.

- [ ] **Step 5: Commit**

```bash
git add packages/frontend/app/src/styles/global.css
git commit -m "feat(design): re-point celerex vars to OmniVyx tokens (app-wide re-skin)"
```

---

## Task 3: Glass CSS utilities + BloomBackdrop component

**Files:**
- Create: `packages/frontend/app/src/styles/omnivyx.css`
- Create: `packages/frontend/app/src/components/omnivyx/BloomBackdrop.tsx`
- Create: `packages/frontend/app/src/components/omnivyx/__tests__/glass.test.tsx`
- Modify: `packages/frontend/app/src/main.tsx`

- [ ] **Step 1: Create `omnivyx.css`**

```css
/* OmniVyx Liquid Glass utilities. Glass must sit over the bloom backdrop. */
.ov-glass {
  background: var(--ov-glass-fill);
  -webkit-backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  border: 1px solid var(--ov-glass-border);
  border-top-color: var(--ov-glass-specular);
  border-radius: var(--ov-radius-lg);
}
.ov-glass-bar {
  background: var(--ov-glass-fill);
  -webkit-backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  border-bottom: 1px solid var(--ov-hairline);
  box-shadow: inset 0 1px 0 var(--ov-glass-specular);
}
.ov-glass-rail {
  background: var(--ov-glass-fill);
  -webkit-backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  backdrop-filter: blur(var(--ov-glass-blur)) saturate(var(--ov-glass-saturate));
  border-right: 1px solid var(--ov-hairline);
}
.ov-bloom-root {
  position: absolute;
  inset: 0;
  overflow: hidden;
  pointer-events: none;
  z-index: 0;
  background: var(--ov-bg-deep);
}
.ov-bloom {
  position: absolute;
  border-radius: 50%;
  filter: blur(60px);
  opacity: 0.5;
}
@media (prefers-reduced-motion: reduce) {
  .ov-bloom { transition: none; }
}
```

- [ ] **Step 2: Create `BloomBackdrop.tsx`**

```tsx
/** Accent-bloom field that Liquid Glass surfaces refract over. */
export function BloomBackdrop() {
  return (
    <div className="ov-bloom-root" data-testid="ov-bloom-backdrop" aria-hidden="true">
      <span
        className="ov-bloom"
        style={{ width: 460, height: 460, top: -180, left: -120, background: "var(--ov-bloom-1)", opacity: 0.42 }}
      />
      <span
        className="ov-bloom"
        style={{ width: 380, height: 380, bottom: -200, right: -120, background: "var(--ov-bloom-2)", opacity: 0.24 }}
      />
    </div>
  );
}
```

- [ ] **Step 3: Import `omnivyx.css`** — in `main.tsx`, after the two token imports from Task 1, add:

```ts
import "./styles/omnivyx.css";
```

- [ ] **Step 4: Write the failing test** — `src/components/omnivyx/__tests__/glass.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { BloomBackdrop } from "../BloomBackdrop";

describe("BloomBackdrop", () => {
  it("renders an aria-hidden bloom field with two blooms", () => {
    const { container } = render(<BloomBackdrop />);
    const root = screen.getByTestId("ov-bloom-backdrop");
    expect(root.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelectorAll(".ov-bloom").length).toBe(2);
  });
});
```

- [ ] **Step 5: Run the test to verify it fails**

Run: `cd packages/frontend/app && bunx vitest run src/components/omnivyx/__tests__/glass.test.tsx`
Expected: FAIL — cannot find module `../BloomBackdrop` (if Step 2 not saved) or assertion error.

- [ ] **Step 6: Confirm BloomBackdrop exists (Step 2) and re-run**

Run: `cd packages/frontend/app && bunx vitest run src/components/omnivyx/__tests__/glass.test.tsx`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add packages/frontend/app/src/styles/omnivyx.css \
        packages/frontend/app/src/components/omnivyx/BloomBackdrop.tsx \
        packages/frontend/app/src/components/omnivyx/__tests__/glass.test.tsx \
        packages/frontend/app/src/main.tsx
git commit -m "feat(design): glass CSS utilities + BloomBackdrop"
```

---

## Task 4: Glass primitives (GlassPanel, GlassButton)

**Files:**
- Create: `packages/frontend/app/src/components/omnivyx/GlassPanel.tsx`
- Create: `packages/frontend/app/src/components/omnivyx/GlassButton.tsx`
- Modify: `packages/frontend/app/src/components/omnivyx/__tests__/glass.test.tsx`

- [ ] **Step 1: Create `GlassPanel.tsx`**

```tsx
import type { CSSProperties, ReactNode } from "react";

export function GlassPanel({
  children,
  style,
  testId,
}: {
  children?: ReactNode;
  style?: CSSProperties;
  testId?: string;
}) {
  return (
    <div className="ov-glass" data-testid={testId} style={{ position: "relative", padding: 14, ...style }}>
      {children}
    </div>
  );
}
```

- [ ] **Step 2: Create `GlassButton.tsx`** (accent glass = tint for prominence)

```tsx
import type { ReactNode } from "react";

export function GlassButton({
  children,
  onClick,
  prominent = false,
  testId,
}: {
  children: ReactNode;
  onClick?: () => void;
  prominent?: boolean;
  testId?: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      className="ov-glass"
      style={{
        padding: "8px 16px",
        borderRadius: "var(--ov-radius)",
        font: "inherit",
        fontSize: 13,
        fontWeight: 500,
        color: "var(--ov-text)",
        cursor: "pointer",
        background: prominent ? "var(--ov-glass-tint)" : "var(--ov-glass-fill)",
      }}
    >
      {children}
    </button>
  );
}
```

- [ ] **Step 3: Append failing tests** to `glass.test.tsx`:

```tsx
import { fireEvent } from "@testing-library/react";
import { GlassPanel } from "../GlassPanel";
import { GlassButton } from "../GlassButton";

describe("Glass primitives", () => {
  it("GlassPanel carries the ov-glass class", () => {
    render(<GlassPanel testId="p">hi</GlassPanel>);
    expect(screen.getByTestId("p").className).toContain("ov-glass");
  });

  it("GlassButton fires onClick and tints when prominent", () => {
    let clicked = false;
    render(
      <GlassButton testId="b" prominent onClick={() => (clicked = true)}>
        Create
      </GlassButton>,
    );
    const btn = screen.getByTestId("b");
    expect(btn.style.background).toContain("ov-glass-tint");
    fireEvent.click(btn);
    expect(clicked).toBe(true);
  });
});
```

- [ ] **Step 4: Run to verify failure, then pass**

Run: `cd packages/frontend/app && bunx vitest run src/components/omnivyx/__tests__/glass.test.tsx`
Expected: first run FAILs on missing modules; after Steps 1–2 saved, re-run → PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add packages/frontend/app/src/components/omnivyx/GlassPanel.tsx \
        packages/frontend/app/src/components/omnivyx/GlassButton.tsx \
        packages/frontend/app/src/components/omnivyx/__tests__/glass.test.tsx
git commit -m "feat(design): GlassPanel + GlassButton primitives"
```

---

## Task 5: Mount the bloom backdrop in the app shell

**Files:**
- Modify: `packages/frontend/app/src/App.tsx:271` (the outer shell `<div>`)

- [ ] **Step 1: Import BloomBackdrop** — at the top of `App.tsx` with the other component imports:

```tsx
import { BloomBackdrop } from "./components/omnivyx/BloomBackdrop";
```

- [ ] **Step 2: Wrap the shell with the bloom** — change the outer container (line 271) from:

```tsx
    <div style={{ display: "flex", height: "100vh", width: "100vw" }}>
```
to:
```tsx
    <div style={{ position: "relative", display: "flex", height: "100vh", width: "100vw", background: "var(--ov-bg)" }}>
      <BloomBackdrop />
```
…and add a matching closing context: ensure the existing children now sit above the bloom by giving the `<main>` and `<Sidebar>` a stacking context. Wrap the existing `{!focusModeActive && <Sidebar .../>}` and `<main>…</main>` so they render after `<BloomBackdrop />` (they already do — just confirm the bloom is the first child). No extra wrapper needed since `.ov-bloom-root` is `z-index:0` and siblings default above it; add `style={{ position: "relative", zIndex: 1 }}` to the `<main>` opening tag (line 273):

```tsx
      <main style={{ position: "relative", zIndex: 1, flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" }}>
```

- [ ] **Step 3: Verify**

- Reload preview, `preview_console_logs` level `error` (none).
- `preview_screenshot` → a soft blue bloom (with a faint magenta hint) is visible behind the shell; content remains legible.

- [ ] **Step 4: Commit**

```bash
git add packages/frontend/app/src/App.tsx
git commit -m "feat(design): mount Liquid Glass bloom backdrop in app shell"
```

---

## Task 6: Glass the top bar (Toolbar)

**Files:**
- Modify: `packages/frontend/app/src/App.tsx` (Toolbar component root, ~line 434–444)

- [ ] **Step 1: Locate the Toolbar root** — the `Toolbar` component's outer `<div>` (around line 434) currently sets `background: "var(--celerex-bg-primary)"`. Add the glass class and drop the solid background. Change the opening tag to include `className="ov-glass-bar"` and remove the `background` line from its `style` object:

```tsx
    <div
      className="ov-glass-bar"
      style={{
        position: "relative",
        zIndex: 2,
        display: "flex",
        alignItems: "center",
        /* (keep the remaining existing style properties EXCEPT background) */
      }}
    >
```

(Preserve every other property already in that style object — height, padding, gap, borderBottom if present; only remove `background` and add `position/zIndex` + the className.)

- [ ] **Step 2: Verify**

- Reload preview, `preview_console_logs` level `error` (none).
- `preview_screenshot` → the top bar is frosted glass with a bright top edge, the bloom subtly showing through.

- [ ] **Step 3: Commit**

```bash
git add packages/frontend/app/src/App.tsx
git commit -m "feat(design): frost the Agent top bar"
```

---

## Task 7: Glass the sidebar rail

**Files:**
- Modify: `packages/frontend/app/src/layout/Sidebar.tsx` (rail root element)

- [ ] **Step 1: Find the Sidebar root** — locate the outermost element of the `Sidebar` component (the `<aside>`/`<nav>`/`<div>` that sets `width: var(--celerex-sidebar-width)` and `background: var(--celerex-bg-sidebar)`).

Run: `cd packages/frontend/app && grep -n "celerex-bg-sidebar\|celerex-sidebar-width" src/layout/Sidebar.tsx | head`

- [ ] **Step 2: Apply the glass rail** — add `className="ov-glass-rail"` to that root element and remove its `background: "var(--celerex-bg-sidebar)"` style property (keep width, borderRight may be replaced by the class's border-right, layout, overflow). Add `position: "relative", zIndex: 1` to its style so it sits above the bloom.

```tsx
      className="ov-glass-rail"
      style={{
        position: "relative",
        zIndex: 1,
        width: "var(--celerex-sidebar-width)",
        /* keep remaining layout properties; remove `background` and any solid `borderRight` */
      }}
```

- [ ] **Step 3: Run the existing Sidebar test (no regression)**

Run: `cd packages/frontend/app && bunx vitest run src/layout/__tests__/Sidebar.test.tsx`
Expected: PASS (the re-skin must not break the add-agent affordance test).

- [ ] **Step 4: Verify visually**

- Reload preview, `preview_console_logs` level `error` (none).
- `preview_screenshot` → sidebar is frosted glass over the bloom; the shell now reads as a Liquid Glass "Studio" sibling of OmniVyx Design. Toggle dark and screenshot once to confirm both themes hold.

- [ ] **Step 5: Full test sweep + commit**

Run: `cd packages/frontend/app && bunx vitest run 2>&1 | tail -15`
Expected: suite green (no new failures vs. baseline).

```bash
git add packages/frontend/app/src/layout/Sidebar.tsx
git commit -m "feat(design): frost the Agent sidebar rail"
```

---

## Self-Review

**Spec coverage:**
- §2 ideology (Studio + Liquid Glass) → Tasks 3–7 (bloom + glass chrome). ✓
- §4 token contract (`omnivyx-tokens.css`, concrete values, light+dark) → Task 1. ✓
- §4.4 per-product accent (Agent blue) → Task 1 theme file. ✓
- §2.3 typography (geometric display + clean sans, Source Serif removed) → Task 1 fonts + Task 2 body font. ✓
- §5 sharing (vendored token contract, contract-version header) → Task 1 file header. ✓
- §6 web glass rules (blur+saturate, specular edge, never opaque behind glass) → Task 3 utilities + Task 5 bloom. ✓
- §7 accessibility fallbacks (`prefers-reduced-transparency` → solid; `prefers-reduced-motion`) → Task 1 + Task 3 media queries. ✓
- §8 rollout step "re-skin Agent first; shell→nav→Settings→dashboards" → this plan does shell + nav (Toolbar/Sidebar); Settings/dashboards/Command-tier tables are an explicit **follow-up plan** (noted). ✓
- §3 material tiers: Glass on chrome only; dense views untouched (calm tier) — honored by limiting glass to shell. Glass-light + Command-tier components are follow-up. ✓ (in scope only as "don't glass dense views").

**Deferred to follow-up plans (not gaps):** Glass-light tier tuning, the Solid-dense "Command" list/table components, per-view re-skins (Settings, Agent Dashboard, Pipeline Editor), self-hosting fonts, and the OmniVyx Design adoption pass (spec §8 step 3).

**Placeholder scan:** No TBD/TODO steps; every code step contains complete code. Font self-hosting is explicitly a documented follow-up, not an in-task placeholder.

**Type consistency:** `BloomBackdrop`, `GlassPanel`, `GlassButton` props/testIds are consistent across creation (Tasks 3–4) and tests. CSS class names (`.ov-glass`, `.ov-glass-bar`, `.ov-glass-rail`, `.ov-bloom-root`, `.ov-bloom`) match between `omnivyx.css` (Task 3) and their consumers (Tasks 5–7). Token names match the spec §4.

**Note for executor:** `index.html` `<title>` still reads "Celerex" (a rebrand residual unrelated to this plan) — leave it; it is tracked separately.
