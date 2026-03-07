import { documentAgents } from "./documentAgents";
import { codevelopmentAgents } from "./codevelopmentAgents";
import { infrastructureAgents } from "./infrastructureAgents";

export interface AgentDef {
  id: string;
  name: string;
  description: string;
  status: "active" | "idle" | "disabled";
  category: "document-management" | "co-development" | "infrastructure-design";
}

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

export const allAgents: AgentDef[] = [
  ...documentAgents,
  ...codevelopmentAgents,
  ...infrastructureAgents,
];

export function getAgentsByCategory(categoryId: CategoryId): AgentDef[] {
  return allAgents.filter((a) => a.category === categoryId);
}

export function searchAgents(query: string, categoryId?: CategoryId): AgentDef[] {
  const q = query.toLowerCase().trim();
  let agents = categoryId ? getAgentsByCategory(categoryId) : allAgents;
  if (!q) return agents;
  return agents.filter(
    (a) =>
      a.name.toLowerCase().includes(q) ||
      a.description.toLowerCase().includes(q) ||
      a.id.includes(q)
  );
}
