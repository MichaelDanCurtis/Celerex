import { NodeCategory } from "./types";

export interface PaletteItem {
  type: string;
  label: string;
  category: NodeCategory;
  defaultConfig: Record<string, unknown>;
}

export const PALETTE: PaletteItem[] = [
  { type: "schedule.tick", label: "Schedule tick", category: "inbound", defaultConfig: {} },
  {
    type: "workiq.pull_metrics",
    label: "workIQ metrics",
    category: "action",
    defaultConfig: { execution: "http", inputs: { window_days: 7 } },
  },
  {
    type: "stargate.query",
    label: "Stargate SQL",
    category: "action",
    defaultConfig: { execution: "http", inputs: { sql: "SELECT 1" } },
  },
  {
    type: "file.extract.xlsx",
    label: "Excel extract",
    category: "action",
    defaultConfig: { execution: "script", inputs: { extraction_mode: "markdown" } },
  },
  {
    type: "file.extract.docx",
    label: "Word extract",
    category: "action",
    defaultConfig: { execution: "script", inputs: { extraction_mode: "markdown" } },
  },
  {
    type: "llm.synthesize",
    label: "LLM synthesize",
    category: "action",
    defaultConfig: { execution: "llm", model: "claude-sonnet-4-6" },
  },
  {
    type: "filter.expr",
    label: "Filter",
    category: "filter",
    defaultConfig: { expression: "true" },
  },
  {
    type: "document.create",
    label: "Create BlockSuite doc",
    category: "outbound",
    defaultConfig: { execution: "script", inputs: { title_template: "Report {{date}}" } },
  },
];

export const CATEGORY_COLORS: Record<NodeCategory, { bg: string; border: string }> = {
  inbound: { bg: "#e0f2fe", border: "#0284c7" },
  action: { bg: "#fef3c7", border: "#d97706" },
  filter: { bg: "#ede9fe", border: "#7c3aed" },
  outbound: { bg: "#dcfce7", border: "#16a34a" },
};
