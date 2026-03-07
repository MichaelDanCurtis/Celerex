import { documentAgents } from "./documentAgents";
import { codevelopmentAgents } from "./codevelopmentAgents";
import { infrastructureAgents } from "./infrastructureAgents";

/* ------------------------------------------------------------------ */
/*  I/O Types                                                         */
/* ------------------------------------------------------------------ */

export const IO_TYPES = [
  { id: "text", label: "Text", color: "#6b7280", icon: "T" },
  { id: "document", label: "Document", color: "#3b82f6", icon: "D" },
  { id: "markdown", label: "Markdown", color: "#8b5cf6", icon: "M" },
  { id: "code", label: "Code", color: "#f59e0b", icon: "{}" },
  { id: "data", label: "Data (JSON/CSV)", color: "#10b981", icon: "[]" },
  { id: "diagram", label: "Diagram / Drawing", color: "#ec4899", icon: "◇" },
  { id: "image", label: "Image", color: "#f97316", icon: "◻" },
  { id: "pdf", label: "PDF", color: "#ef4444", icon: "P" },
  { id: "html", label: "HTML", color: "#06b6d4", icon: "<>" },
  { id: "any", label: "Any", color: "#9ca3af", icon: "*" },
] as const;

export type IOTypeId = (typeof IO_TYPES)[number]["id"];

export function getIOType(id: string) {
  return IO_TYPES.find((t) => t.id === id) ?? IO_TYPES[0];
}

/* ------------------------------------------------------------------ */
/*  Agent Definition                                                  */
/* ------------------------------------------------------------------ */

export interface AgentDef {
  id: string;
  name: string;
  description: string;
  status: "active" | "idle" | "disabled";
  category: "document-management" | "co-development" | "infrastructure-design";
  /** What this agent accepts as input */
  inputs?: IOTypeId[];
  /** What this agent produces as output */
  outputs?: IOTypeId[];
  /** System prompt / instructions for the agent */
  systemPrompt?: string;
  /** Which AI model to use (overrides default) */
  model?: string;
  /** Tool IDs this agent can invoke */
  tools?: string[];
  /** Whether this is a user-created custom agent */
  isCustom?: boolean;
}

/* ------------------------------------------------------------------ */
/*  Categories                                                        */
/* ------------------------------------------------------------------ */

export const CATEGORIES = [
  {
    id: "document-management" as const,
    label: "Document Management",
    description: "Document lifecycle, creation, formatting, analysis, and content services",
  },
  {
    id: "co-development" as const,
    label: "Co-Development",
    description: "Software development, coding, testing, DevOps, and collaborative engineering",
  },
  {
    id: "infrastructure-design" as const,
    label: "Infrastructure Design",
    description: "Cloud architecture, networking, security, monitoring, and system design",
  },
] as const;

export type CategoryId = (typeof CATEGORIES)[number]["id"];

/* ------------------------------------------------------------------ */
/*  Default I/O assignments by category                               */
/* ------------------------------------------------------------------ */

const DEFAULT_IO: Record<CategoryId, { inputs: IOTypeId[]; outputs: IOTypeId[] }> = {
  "document-management": { inputs: ["text", "document"], outputs: ["document", "text"] },
  "co-development": { inputs: ["code", "text"], outputs: ["code", "text"] },
  "infrastructure-design": { inputs: ["text", "data"], outputs: ["diagram", "text", "code"] },
};

function withDefaults(agent: AgentDef): AgentDef {
  if (agent.inputs && agent.outputs) return agent;
  const defaults = DEFAULT_IO[agent.category];
  return {
    ...agent,
    inputs: agent.inputs ?? defaults.inputs,
    outputs: agent.outputs ?? defaults.outputs,
  };
}

/* ------------------------------------------------------------------ */
/*  Built-in agents (with default I/O applied)                        */
/* ------------------------------------------------------------------ */

const builtInAgents: AgentDef[] = [
  ...documentAgents,
  ...codevelopmentAgents,
  ...infrastructureAgents,
].map(withDefaults);

/* ------------------------------------------------------------------ */
/*  Custom agents store (persisted to localStorage)                   */
/* ------------------------------------------------------------------ */

const CUSTOM_AGENTS_KEY = "celerex-custom-agents";

function loadCustomAgents(): AgentDef[] {
  try {
    const raw = localStorage.getItem(CUSTOM_AGENTS_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveCustomAgents(agents: AgentDef[]) {
  localStorage.setItem(CUSTOM_AGENTS_KEY, JSON.stringify(agents));
}

let customAgents: AgentDef[] = loadCustomAgents();

/* ------------------------------------------------------------------ */
/*  Public API                                                        */
/* ------------------------------------------------------------------ */

/** All agents (built-in + custom) */
export function getAllAgents(): AgentDef[] {
  return [...builtInAgents, ...customAgents];
}

/** For backward compat — static reference (won't include dynamically added custom agents) */
export const allAgents: AgentDef[] = builtInAgents;

export function getAgentsByCategory(categoryId: CategoryId): AgentDef[] {
  return getAllAgents().filter((a) => a.category === categoryId);
}

export function searchAgents(query: string, categoryId?: CategoryId): AgentDef[] {
  const q = query.toLowerCase().trim();
  let agents = categoryId ? getAgentsByCategory(categoryId) : getAllAgents();
  if (!q) return agents;
  return agents.filter(
    (a) =>
      a.name.toLowerCase().includes(q) ||
      a.description.toLowerCase().includes(q) ||
      a.id.includes(q)
  );
}

export function getAgentById(id: string): AgentDef | undefined {
  return getAllAgents().find((a) => a.id === id);
}

export function addCustomAgent(agent: AgentDef): void {
  customAgents = [...customAgents, { ...agent, isCustom: true }];
  saveCustomAgents(customAgents);
}

export function updateCustomAgent(id: string, updates: Partial<AgentDef>): void {
  customAgents = customAgents.map((a) => (a.id === id ? { ...a, ...updates } : a));
  saveCustomAgents(customAgents);
}

export function deleteCustomAgent(id: string): void {
  customAgents = customAgents.filter((a) => a.id !== id);
  saveCustomAgents(customAgents);
}

/** Check if two I/O types are compatible for connection */
export function areTypesCompatible(outputTypes: IOTypeId[], inputTypes: IOTypeId[]): boolean {
  if (outputTypes.includes("any") || inputTypes.includes("any")) return true;
  return outputTypes.some((o) => inputTypes.includes(o));
}
