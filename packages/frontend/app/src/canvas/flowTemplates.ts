import { Graph } from "./types";

export interface FlowTemplate {
  id: string;
  name: string;
  description: string;
  icon: string;
  /** Returns a fresh graph each call so callers can mutate freely. */
  build: () => Graph;
}

const SONNET = "claude-sonnet-4-6";

function llm(
  id: string,
  label: string,
  system: string,
  user: string,
  x: number,
  y: number,
): Graph["nodes"][number] {
  return {
    id,
    type: "action",
    position: { x, y },
    config: {
      execution: "llm",
      template: "llm.synthesize",
      label,
      model: SONNET,
      provider_id: "default",
      inputs: { messages: [{ role: "system", content: system }, { role: "user", content: user }] },
    },
  };
}

function trigger(id: string, label: string, x: number, y: number): Graph["nodes"][number] {
  return {
    id,
    type: "inbound",
    position: { x, y },
    config: { execution: "input", template: "manual.trigger", label, inputs: { triggered: true } },
  };
}

function docCreate(id: string, label: string, title: string, contentRef: string, x: number, y: number): Graph["nodes"][number] {
  return {
    id,
    type: "outbound",
    position: { x, y },
    config: {
      execution: "doc",
      op: "create",
      template: "doc.create",
      label,
      title,
      inputs: { markdown: contentRef },
    },
  };
}

function docAppend(id: string, label: string, docRef: string, contentRef: string, x: number, y: number): Graph["nodes"][number] {
  return {
    id,
    type: "outbound",
    position: { x, y },
    config: {
      execution: "doc",
      op: "append",
      template: "doc.append",
      label,
      document_id: docRef,
      inputs: { markdown: contentRef },
    },
  };
}

export const FLOW_TEMPLATES: FlowTemplate[] = [
  {
    id: "blank",
    name: "Blank flow",
    description: "Start from an empty canvas.",
    icon: "✦",
    build: () => ({ nodes: [], edges: [] }),
  },
  {
    id: "newsletter",
    name: "Jean's Newsletter",
    description: "Schedule → write a newsletter with an LLM → publish to a document.",
    icon: "📰",
    build: () => ({
      nodes: [
        trigger("trigger", "Weekly schedule", 40, 160),
        llm(
          "agent",
          "Write newsletter",
          "You are Jean's newsletter editor. Write a warm, concise weekly newsletter in Markdown with a title, 3 short sections, and a sign-off.",
          "Draft this week's newsletter covering product updates and community highlights.",
          330,
          160,
        ),
        docCreate("doc", "Publish", "Weekly Newsletter", "{{steps.agent.outputs}}", 640, 160),
      ],
      edges: [
        { from: "trigger", to: "agent" },
        { from: "agent", to: "doc" },
      ],
    }),
  },
  {
    id: "researcher",
    name: "Bob the Researcher",
    description: "Run on demand → research a topic with an LLM → save a research report.",
    icon: "🔬",
    build: () => ({
      nodes: [
        trigger("trigger", "Run research", 40, 160),
        llm(
          "agent",
          "Research & analyze",
          "You are a rigorous research analyst. Produce a structured Markdown brief with an executive summary, key findings (bulleted), and open questions.",
          "Research the current state of agentic document automation and summarize the landscape.",
          330,
          160,
        ),
        docCreate("doc", "Save report", "Research Report", "{{steps.agent.outputs}}", 640, 160),
      ],
      edges: [
        { from: "trigger", to: "agent" },
        { from: "agent", to: "doc" },
      ],
    }),
  },
  {
    id: "ideation",
    name: "Ideation → Docs",
    description: "Brainstorm ideas with an LLM and drop them straight into a document.",
    icon: "💡",
    build: () => ({
      nodes: [
        trigger("trigger", "New idea", 40, 160),
        llm(
          "agent",
          "Brainstorm",
          "You are a creative product strategist. Generate 10 concrete, distinct ideas as a Markdown numbered list, each with a one-line rationale.",
          "Brainstorm features for a document-centric agentic workspace.",
          330,
          160,
        ),
        docCreate("doc", "Capture ideas", "Idea Backlog", "{{steps.agent.outputs}}", 640, 160),
      ],
      edges: [
        { from: "trigger", to: "agent" },
        { from: "agent", to: "doc" },
      ],
    }),
  },
  {
    id: "docbuilder",
    name: "Doc Builder (chained)",
    description: "Outline → create a doc → expand a section → append it back to the SAME doc.",
    icon: "🏗️",
    build: () => ({
      nodes: [
        trigger("trigger", "Start", 40, 220),
        llm(
          "outline",
          "Draft outline",
          "You write tight document outlines.",
          "Produce a 4-section Markdown outline for a brief on 'agentic automation for business users'.",
          300,
          120,
        ),
        docCreate("create", "Create brief", "Agentic Automation Brief", "{{steps.outline.outputs}}", 580, 120),
        llm(
          "expand",
          "Expand section",
          "You expand outlines into full prose, Markdown formatted.",
          "Expand the first section of this outline into 2 detailed paragraphs:\n\n{{steps.outline.outputs}}",
          300,
          340,
        ),
        docAppend(
          "append",
          "Append section",
          "{{steps.create.outputs.document_id}}",
          "{{steps.expand.outputs}}",
          860,
          220,
        ),
      ],
      edges: [
        { from: "trigger", to: "outline" },
        { from: "outline", to: "create" },
        { from: "outline", to: "expand" },
        { from: "create", to: "append" },
        { from: "expand", to: "append" },
      ],
    }),
  },
];

export function flowTemplateById(id: string): FlowTemplate | undefined {
  return FLOW_TEMPLATES.find((t) => t.id === id);
}
