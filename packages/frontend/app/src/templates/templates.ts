export interface DocTemplate {
  id: string;
  name: string;
  description: string;
  category: "consulting" | "general" | "research";
  titlePrefix: string;
}

export const templates: DocTemplate[] = [
  { id: "blank", name: "Blank Document", description: "Start from scratch", category: "general", titlePrefix: "Untitled" },
  { id: "meeting-notes", name: "Meeting Notes", description: "Structure for meeting agendas and action items", category: "general", titlePrefix: "Meeting Notes" },
  { id: "project-brief", name: "Project Brief", description: "Define project scope, goals, and deliverables", category: "consulting", titlePrefix: "Project Brief" },
  { id: "research-report", name: "Research Report", description: "Structured research findings and analysis", category: "research", titlePrefix: "Research Report" },
  { id: "executive-summary", name: "Executive Summary", description: "Concise overview for stakeholders", category: "consulting", titlePrefix: "Executive Summary" },
  { id: "weekly-status", name: "Weekly Status Update", description: "Progress tracking and blockers", category: "general", titlePrefix: "Status Update" },
  { id: "technical-spec", name: "Technical Specification", description: "System design and architecture documentation", category: "research", titlePrefix: "Technical Spec" },
  { id: "stakeholder-analysis", name: "Stakeholder Analysis", description: "Map key stakeholders and their interests", category: "consulting", titlePrefix: "Stakeholder Analysis" },
];
