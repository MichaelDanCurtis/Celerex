# Multi-Document with Engagements

## Data Model

Engagement { id, name, createdAt, docs[] }
DocMeta { id, title, engagementId, createdAt }

State via Jotai atoms. BlockSuite TestWorkspace manages actual Yjs docs.

## Sidebar

- Engagements are collapsible sections
- Docs listed under their engagement, active doc highlighted
- Inline "+" to create engagements and docs (type name, Enter)
- Default "Personal" engagement on first load

## Editor Integration

- Active doc ID atom drives EditorArea → BlockSuiteEditor
- Doc title syncs from BlockSuite title block back to sidebar

## Files

- `src/state/atoms.ts` — Jotai atoms
- `src/layout/Sidebar.tsx` — rewrite with dynamic list
- `src/layout/EditorArea.tsx` — read active doc from atom
- `src/editor/BlockSuiteEditor.tsx` — minor cleanup

## Out of Scope

- Persistence, drag-drop, move between engagements, context menus
