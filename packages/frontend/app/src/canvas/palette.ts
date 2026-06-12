import { NodeCategory } from "./types";

export interface PaletteItem {
  /** Stable template id, e.g. "llm.synthesize". Stored in node config as `template`. */
  type: string;
  label: string;
  category: NodeCategory;
  /** Emoji glyph shown on the node + palette. */
  icon: string;
  /** One-line description for the palette. */
  description: string;
  /** Per-item accent colour; falls back to the category colour. */
  accent?: string;
  /** Seed config applied when the node is dropped/added. */
  defaultConfig: Record<string, unknown>;
}

export interface PaletteGroup {
  category: NodeCategory;
  title: string;
  items: PaletteItem[];
}

// ---- Category visual system (tuned for the dark canvas) -------------------

export interface CategoryMeta {
  color: string;
  glow: string;
  label: string;
  icon: string;
}

export const CATEGORY_META: Record<NodeCategory, CategoryMeta> = {
  inbound: { color: "#22d3ee", glow: "rgba(34,211,238,0.35)", label: "Trigger", icon: "⚡" },
  action: { color: "#a78bfa", glow: "rgba(167,139,250,0.35)", label: "Action", icon: "✦" },
  filter: { color: "#f472b6", glow: "rgba(244,114,182,0.35)", label: "Filter", icon: "⑂" },
  outbound: { color: "#34d399", glow: "rgba(52,211,153,0.35)", label: "Output", icon: "→" },
};

/** Back-compat shape some older code referenced. */
export const CATEGORY_COLORS: Record<NodeCategory, { bg: string; border: string }> = {
  inbound: { bg: "#0c2a33", border: CATEGORY_META.inbound.color },
  action: { bg: "#211a38", border: CATEGORY_META.action.color },
  filter: { bg: "#331226", border: CATEGORY_META.filter.color },
  outbound: { bg: "#0c2e22", border: CATEGORY_META.outbound.color },
};

// ---- The palette ----------------------------------------------------------

export const PALETTE_GROUPS: PaletteGroup[] = [
  {
    category: "inbound",
    title: "Triggers",
    items: [
      {
        type: "manual.trigger",
        label: "Manual trigger",
        category: "inbound",
        icon: "▶",
        description: "Start the flow on demand with a Run.",
        defaultConfig: { execution: "input", inputs: { triggered: true } },
      },
      {
        type: "schedule.tick",
        label: "Schedule",
        category: "inbound",
        icon: "⏰",
        description: "Entry point for cron-scheduled runs.",
        defaultConfig: { execution: "input", inputs: { triggered: true } },
      },
      {
        type: "webhook.in",
        label: "Webhook",
        category: "inbound",
        icon: "🪝",
        description: "Fire when an inbound webhook arrives.",
        defaultConfig: { execution: "input", inputs: {} },
      },
    ],
  },
  {
    category: "action",
    title: "Agents & compute",
    items: [
      {
        type: "llm.synthesize",
        label: "LLM agent",
        category: "action",
        icon: "🧠",
        accent: "#a78bfa",
        description: "Reason / write with a chosen model.",
        defaultConfig: {
          execution: "llm",
          model: "claude-sonnet-4-6",
          provider_id: "default",
          inputs: {
            messages: [
              { role: "user", content: "Write a short note about: {{steps.trigger.outputs}}" },
            ],
          },
        },
      },
      {
        type: "doc.read",
        label: "Read document",
        category: "action",
        icon: "📖",
        accent: "#34d399",
        description: "Read a document's text for downstream nodes.",
        defaultConfig: { execution: "doc", op: "read", document_id: "" },
      },
      {
        type: "http.request",
        label: "HTTP request",
        category: "action",
        icon: "🌐",
        accent: "#60a5fa",
        description: "Call any REST endpoint.",
        defaultConfig: { execution: "http", url: "https://", method: "GET", headers: {}, inputs: {} },
      },
      {
        type: "script.run",
        label: "Run script",
        category: "action",
        icon: "⚡",
        accent: "#94a3b8",
        description: "Run a local command.",
        defaultConfig: { execution: "script", command: "echo", args: ["hello from Celerex"] },
      },
    ],
  },
  {
    category: "action",
    title: "Connectors",
    items: [
      {
        type: "workiq.pull_metrics",
        label: "workIQ metrics",
        category: "action",
        icon: "📊",
        accent: "#60a5fa",
        description: "Pull engagement metrics (workIQ plugin).",
        defaultConfig: {
          execution: "http",
          url: "https://workiq.example/api/metrics",
          method: "POST",
          inputs: { window_days: 7 },
        },
      },
      {
        type: "stargate.query",
        label: "Stargate SQL",
        category: "action",
        icon: "🛰️",
        accent: "#60a5fa",
        description: "Query a warehouse (Stargate plugin).",
        defaultConfig: {
          execution: "http",
          url: "https://stargate.example/api/query",
          method: "POST",
          inputs: { sql: "SELECT 1" },
        },
      },
    ],
  },
  {
    category: "action",
    title: "Memory & federation",
    items: [
      {
        type: "memory.search",
        label: "Memory: recall",
        category: "action",
        icon: "🔎",
        accent: "#2dd4bf",
        description: "Semantic recall from galvo-memory (Neo4j).",
        defaultConfig: {
          execution: "memory",
          op: "search",
          mem_label: "Entity",
          query: "{{steps.trigger.outputs}}",
          limit: 5,
        },
      },
      {
        type: "memory.store",
        label: "Memory: store",
        category: "action",
        icon: "🗃️",
        accent: "#2dd4bf",
        description: "Write a typed memory node to galvo-memory.",
        defaultConfig: {
          execution: "memory",
          op: "store",
          mem_label: "Decision",
          fields: { title: "", rationale: "", confidence: 0.8 },
        },
      },
      {
        type: "galvo.invoke",
        label: "Galvo agent",
        category: "action",
        icon: "🛰️",
        accent: "#818cf8",
        description: "Invoke a Galvo agent via its control plane.",
        defaultConfig: { execution: "galvo", op: "invoke", agent: "", input: "{{steps.trigger.outputs}}" },
      },
      {
        type: "m365.fetch",
        label: "M365 fetch",
        category: "action",
        icon: "📥",
        accent: "#38bdf8",
        description: "Read Microsoft Graph (mail, calendar, SharePoint).",
        defaultConfig: { execution: "m365", op: "graph_get", path: "/users/{user}/messages" },
      },
      {
        type: "m365.publish",
        label: "M365 Copilot publish",
        category: "outbound",
        icon: "Ⓜ️",
        accent: "#38bdf8",
        description: "Publish a document into Microsoft 365 Copilot.",
        defaultConfig: {
          execution: "m365",
          op: "copilot_publish",
          connection_id: "celerexdocs",
          item_id: "doc-1",
          title: "Celerex document",
          content: "{{steps.agent.outputs}}",
        },
      },
    ],
  },
  {
    category: "filter",
    title: "Logic",
    items: [
      {
        type: "filter.expr",
        label: "Filter / gate",
        category: "filter",
        icon: "⑂",
        description: "Gate downstream nodes on a condition.",
        defaultConfig: { expression: "{{steps.trigger.outputs.triggered}} == true" },
      },
      {
        type: "review.approve",
        label: "Review & approve",
        category: "filter",
        icon: "✅",
        accent: "#fbbf24",
        description: "Pause for a human to approve/edit before continuing.",
        defaultConfig: {
          execution: "review",
          prompt: "Review and approve this draft",
          content: "{{steps.agent.outputs}}",
        },
      },
    ],
  },
  {
    category: "outbound",
    title: "Documents & output",
    items: [
      {
        type: "doc.create",
        label: "Create document",
        category: "outbound",
        icon: "📄",
        description: "Create a new document from upstream content.",
        defaultConfig: {
          execution: "doc",
          op: "create",
          title: "Generated report",
          inputs: { markdown: "{{steps.agent.outputs}}" },
        },
      },
      {
        type: "doc.append",
        label: "Append to document",
        category: "outbound",
        icon: "➕",
        description: "Append a block to an existing document.",
        defaultConfig: {
          execution: "doc",
          op: "append",
          document_id: "{{steps.create.outputs.document_id}}",
          inputs: { markdown: "{{steps.agent.outputs}}" },
        },
      },
      {
        type: "http.post",
        label: "Send webhook",
        category: "outbound",
        icon: "📡",
        accent: "#60a5fa",
        description: "POST results to an external URL.",
        defaultConfig: { execution: "http", url: "https://", method: "POST", inputs: {} },
      },
    ],
  },
];

/** Flat list (kept for back-compat with code that iterates PALETTE). */
export const PALETTE: PaletteItem[] = PALETTE_GROUPS.flatMap((g) => g.items);

export function paletteItemByType(type: string): PaletteItem | undefined {
  return PALETTE.find((p) => p.type === type);
}

export function accentFor(category: NodeCategory, config?: Record<string, unknown>): string {
  const template = config?.template as string | undefined;
  const item = template ? paletteItemByType(template) : undefined;
  return item?.accent ?? CATEGORY_META[category].color;
}

export function iconFor(category: NodeCategory, config?: Record<string, unknown>): string {
  const template = config?.template as string | undefined;
  const item = template ? paletteItemByType(template) : undefined;
  return item?.icon ?? CATEGORY_META[category].icon;
}
