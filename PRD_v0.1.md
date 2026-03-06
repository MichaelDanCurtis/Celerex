# PRODUCT REQUIREMENTS DOCUMENT

# Celerex
**The Agentic Workspace for Building, Operating, and Collaborating at Scale**

---

Version 0.1 | March 2026 | DRAFT

Author: Michael Curtis

Classification: Confidential

| Field | Detail |
|-------|--------|
| Document Owner | Michael Curtis |
| Status | Draft — Iteration 1 |
| Target Users | Development teams, consultants, platform engineers, technical leaders |
| Primary Domain | Software Development, Infrastructure, AI/ML, DevOps, Technical Consulting |
| Deployment | SaaS (Cloud-hosted, multi-tenant) with self-host option |
| Tech Stack (Proposed) | Bun-based frontend, Rust backend |

---

## 1. Executive Summary

Celerex is a new platform that recreates and extends what **AFFiNE** does. AFFiNE (MIT-licensed) serves as the primary reference implementation — we clone the repo as a reference, study its architecture, and migrate the patterns, components, and code we need into our own clean codebase built on our chosen stack (Bun frontend, Rust backend).

We are **not forking AFFiNE**. We are building Celerex from scratch, using AFFiNE as a blueprint for the workspace experience. Key pieces we migrate or reimplement from AFFiNE:

- **BlockSuite patterns**: The block-based editor architecture, block schemas, the command system, inline editing model. We may use BlockSuite directly as a dependency (it's a standalone npm package) or reimplement its patterns in our stack.
- **Edgeless + Page dual-mode canvas**: The core UX of seamlessly switching between spatial canvas and structured document mode.
- **CRDT data layer**: AFFiNE uses Yjs (JS) and y-octo (Rust). We adopt the same CRDT approach, likely using y-octo directly since our backend is Rust.
- **Local-first sync model**: Data lives locally first, syncs to cloud. Same philosophy, rebuilt for our architecture.
- **Core block types**: Rich text, databases, embeds, drawing surface — we start from AFFiNE's implementations and adapt.

What AFFiNE does well, we keep. What it doesn't do, we add:

1. **Agentic development orchestration** — a harness for coordinating fleets of AI coding agents across large projects, managing multi-repo development workflows through GitHub, orchestrating infrastructure-as-code deployments, and automating complex multi-step engineering tasks.

2. **ConsultantCanvas engagement intelligence** — deep AI co-piloting, client collaboration portals, engagement lifecycle management, plugin-based diagnostic tooling, and professional deliverable generation.

3. **VS Code integration** — workspace outputs (code, configs, IaC, generated projects) open directly in VS Code. Agent-produced code, terminal sessions, and file trees bridge seamlessly between the Celerex canvas and a full development environment.

Celerex adds these layers beyond AFFiNE's foundation:

1. **Agentic development orchestration** — a harness for coordinating fleets of AI coding agents across large projects, managing multi-repo development workflows through GitHub, orchestrating infrastructure-as-code deployments, and automating complex multi-step engineering tasks.

2. **ConsultantCanvas engagement intelligence** — deep AI co-piloting, client collaboration portals, engagement lifecycle management, plugin-based diagnostic tooling, and professional deliverable generation.

3. **VS Code integration** — workspace outputs (code, configs, IaC, generated projects) open directly in VS Code. Agent-produced code, terminal sessions, and file trees bridge seamlessly between the Celerex canvas and a full development environment.

The result is AFFiNE's beloved workspace experience — the canvas, the docs, the local-first data ownership — combined with the ability to orchestrate AI agents, manage client engagements, generate professional deliverables, and push outputs straight into VS Code for hands-on development.

Celerex is not another project management tool. It is an **operating environment** — the place where knowledge work, engineering work, and agentic automation converge.

---

## 2. Problem Statement

### 2.1 The Three-Way Fragmentation

Today's technical teams live across three disconnected worlds:

- **Knowledge & collaboration tools** (Notion, Confluence, Miro, Google Docs) — where thinking happens, but it's disconnected from execution.
- **Development & operations tools** (GitHub, terminals, CI/CD dashboards, cloud consoles) — where execution happens, but it's disconnected from context.
- **AI assistants** (ChatGPT, Claude, Copilot) — used in yet another window, with no persistent memory of the project, no access to the full picture, and no ability to act autonomously.

The result: context is scattered, knowledge is siloed, and AI is hobbled by lack of integration.

### 2.2 The Agentic Gap

AI coding agents (Claude Code, Codex, Devin, Sweep) can now write code, fix bugs, and implement features. But there is no unified **orchestration layer** for:

- Coordinating multiple agents working on different parts of a large project simultaneously
- Connecting agent work to the broader project context (architecture decisions, client requirements, infrastructure constraints)
- Reviewing, approving, and steering agent output within the same workspace where planning happens
- Managing the lifecycle of agentic tasks — from ideation on a whiteboard to implementation by agents to deployment via CI/CD

### 2.3 The Collaboration Gap

Consultants and technical leaders need to share work with clients and stakeholders who don't live in GitHub or terminals. Current solutions force a choice: use developer tools (exclude non-technical stakeholders) or use collaboration tools (disconnect from engineering reality).

---

## 3. Vision & Design Principles

### 3.1 Product Vision

Celerex is the operating environment where teams think, build, and ship — with AI agents as first-class team members. It is where a whiteboard sketch becomes an architecture document, becomes a set of agent tasks, becomes deployed infrastructure, becomes a client deliverable — all without leaving the workspace.

### 3.2 Design Principles

| Principle | Description |
|-----------|-------------|
| **AFFiNE is the foundation** | We don't reinvent the workspace. AFFiNE's BlockSuite, edgeless canvas, page editor, and CRDT sync are proven. We build on them directly, migrating code and using packages from the MIT-licensed AFFiNE project. |
| **Canvas-native** | Everything lives on the edgeless canvas. Documents, diagrams, agent dashboards, terminal sessions, and data views are all blocks that can be arranged spatially or structured into pages. Same dual-mode UX as AFFiNE. |
| **Agents are teammates** | AI agents are not chat sidekicks. They are visible participants with their own task queues, work logs, and outputs — managed from the same workspace as human work. |
| **VS Code is the editor** | Celerex is for thinking, planning, orchestrating, and collaborating. VS Code is for hands-on coding. The two are deeply linked — outputs flow to VS Code, activity flows back to Celerex. |
| **Local-first, cloud-synced** | Data lives on your machine first (CRDT-based), syncs to the cloud for collaboration. You own your data. Works offline. Same philosophy as AFFiNE. |
| **Plugin-extensible** | The core platform is lean. Domain capabilities (K8s diagnostics, cloud ops, ML pipelines, consulting workflows) are delivered as plugins. |
| **Context is king** | The AI layer has access to the full workspace — every document, diagram, agent output, git commit, and conversation. It connects dots humans miss. |
| **Stakeholder-inclusive** | Non-technical collaborators (clients, executives, PMs) have their own views into the workspace — seeing progress, deliverables, and evidence without needing to understand the tooling underneath. |

---

## 4. What Celerex Looks Like

### 4.1 The Workspace

The primary interface is an **edgeless canvas workspace** — a zoomable, pannable infinite surface where all content types coexist:

```
+------------------------------------------------------------------+
|  [Workspace Nav]  [Search]  [Agents]  [Plugins]     [Share]      |
+------------------------------------------------------------------+
|                                                                   |
|  +------------------+    +-------------------+                    |
|  | Architecture     |    | Sprint Board      |                   |
|  | Diagram          |    | (Database View)   |                   |
|  | (Excalidraw)     |    |                   |                   |
|  |                  |    | [ ] Auth service   |                   |
|  |   [API]-->[DB]   |    | [ ] Agent: refactor|                  |
|  |    |             |    | [x] Deploy staging |                   |
|  +------------------+    +-------------------+                    |
|                                                                   |
|  +------------------+    +-------------------+                    |
|  | PRD Document     |    | Agent Fleet       |                   |
|  | (Rich Text)      |    | (Live Dashboard)  |                   |
|  |                  |    |                   |                   |
|  | ## Requirements  |    | Agent-1: auth svc  |                   |
|  | The system must..|    |   status: coding   |                   |
|  |                  |    | Agent-2: tests     |                   |
|  +------------------+    |   status: PR open  |                   |
|                          +-------------------+                    |
|                                                       [AI Panel]  |
|  +------------------+    +-------------------+    +-------------+ |
|  | Terminal Session  |    | Client Portal    |    | Claude       | |
|  | (Diagnostic)     |    | Preview          |    |              | |
|  |                  |    |                   |    | "The auth    | |
|  | $ kubectl get po |    | What your client  |    | diagram shows| |
|  | NAME     READY   |    | will see...       |    | a SPOF at..."| |
|  +------------------+    +-------------------+    +-------------+ |
+------------------------------------------------------------------+
```

Any block can be:
- **Expanded** into a full page for focused work
- **Linked** to other blocks (the AI tracks these relationships)
- **Embedded** inside documents or other blocks
- **Shared** with specific collaborators at configurable permission levels
- **Assigned** to an AI agent for autonomous work

### 4.2 Content Block Types

| Block Type | Description |
|-----------|-------------|
| **Rich Text** | Block-based document editor (like Notion). Supports headings, lists, tables, code blocks, callouts, toggles. Real-time collaborative editing via CRDTs. |
| **Canvas / Whiteboard** | Excalidraw-powered freeform drawing surface. Architecture diagrams, flowcharts, sketches. AI can interpret and generate from these. |
| **Database** | Structured data views — tables, kanban boards, calendars, galleries. Used for task tracking, engagement management, asset inventories. |
| **Terminal** | Live terminal session (SSH, local shell, kubectl, cloud CLIs). Commands and outputs are captured and indexed by the AI. |
| **Agent Card** | Live view of an AI agent's current task, status, progress, and outputs. Click to expand into full agent workspace. |
| **Code Editor** | Embedded code viewing/editing with syntax highlighting. Linked to git repos. |
| **Embed** | External content — GitHub PRs, CI/CD pipelines, monitoring dashboards, Figma frames, API docs. |
| **AI Chat** | Contextual AI conversation scoped to the current workspace, page, or block. |
| **Data Viz** | Charts, graphs, and metrics rendered from structured data or external sources. |

---

## 5. Core Platform Capabilities

### 5.1 Knowledge Canvas (migrated from AFFiNE)

The workspace foundation replicates AFFiNE's core experience. These are not "inspired by" features — they are the same capabilities, rebuilt or directly migrated from AFFiNE's MIT-licensed codebase into the Celerex stack.

#### 5.1.1 What We Take from AFFiNE

- **Edgeless mode**: Infinite canvas where all blocks float. Zoom, pan, group, connect with arrows, freeform spatial arrangement. AFFiNE's EdgelessEditor is the reference.
- **Page mode**: Blocks arranged vertically as a structured document. AFFiNE's PageEditor is the reference.
- **Seamless switching**: Toggle between edgeless and page mode for the same content — the signature AFFiNE UX.
- **BlockSuite block system**: The block schema model, command system (`@blocksuite/block-std`), inline editing (`@blocksuite/inline`), and store layer (`@blocksuite/store`). We either use BlockSuite as a direct dependency or reimplement its architecture.
- **CRDT sync**: Yjs on the frontend, y-octo (Rust) on the backend. Conflict-free real-time collaboration.
- **Local-first data**: All writes go to local storage first. Background sync to cloud. Full offline support.
- **Core blocks**: Rich text, headings, lists, tables, code blocks, callouts, toggles, images, embeds, database views, drawing surface.

#### 5.1.2 What We Extend

- **Version history with AI summaries**: Every change is tracked (CRDT gives us this for free). The AI generates human-readable changelogs between versions.
- **Cross-linking & knowledge graph**: Any block can reference any other. The AI maintains and visualizes a knowledge graph of relationships across the workspace.
- **New block types**: Terminal, Agent Card, Code Editor, CI/CD embed, Data Viz — see Section 4.2. These extend BlockSuite's block model with Celerex-specific capabilities.

### 5.2 Agentic Development Orchestration (New)

This is the differentiating capability — Celerex as a **command center for AI coding agents**.

#### 5.2.1 Agent Fleet Management

Celerex can spawn, monitor, and coordinate multiple AI coding agents working on a project simultaneously. Think of it as an air traffic control tower for agentic development.

- **Task decomposition**: Take a high-level objective (from a document, whiteboard, or conversation) and break it into discrete agent tasks. The AI suggests decomposition; the human approves.
- **Agent assignment**: Each task is assigned to an agent instance with a scoped context — relevant files, architecture docs, constraints, and acceptance criteria drawn from the workspace.
- **Parallel execution**: Multiple agents work simultaneously on independent tasks. The orchestrator detects conflicts (overlapping file edits) and serializes or mediates.
- **Progress visibility**: Each agent's work is visible in real-time — what files it's reading, what code it's writing, what questions it has. Agent cards on the canvas show live status.
- **Human-in-the-loop gates**: Configurable checkpoints where agents pause for human review before proceeding. Critical for architecture decisions, security-sensitive code, and external API integrations.
- **PR-based integration**: Agent output flows through standard GitHub PR workflows. Agents create branches, write code, open PRs, respond to review comments. Humans review and merge.

#### 5.2.2 Orchestration Strategies

| Strategy | Description | Use Case |
|----------|-------------|----------|
| **Parallel Fan-out** | Decompose a feature into independent sub-tasks, assign each to an agent, merge results | Large feature implementation across multiple files/services |
| **Pipeline** | Sequential agents — one's output feeds the next | Design → implement → test → document |
| **Specialist Swarm** | Different agents with different expertise work on the same codebase | Frontend agent + backend agent + infra agent |
| **Review Chain** | One agent writes, another reviews, findings go back for revision | High-quality code with automated review cycles |
| **Scout → Build** | A research agent explores options and makes recommendations, a build agent implements the chosen approach | Technology selection, API integration |

#### 5.2.3 Context Injection

The key innovation: agents receive context from the workspace, not just from the codebase.

- Architecture diagrams from the canvas are translated into constraints for the agent
- PRD sections become acceptance criteria
- Previous agent outputs and review feedback become learning context
- Infrastructure state (from diagnostic plugins) informs what the agent can assume about the environment

#### 5.2.4 GitHub Deep Integration

- **Multi-repo orchestration**: Agents work across multiple repositories in a coordinated way
- **Branch strategy management**: The orchestrator manages branch naming, creation, and lifecycle
- **PR dashboards**: All agent-created PRs visible in the workspace with AI-generated summaries
- **CI/CD feedback loop**: Build failures feed back to agents for self-correction
- **Issue → Agent pipeline**: GitHub issues can be directly assigned to agents with one click

### 5.3 AI Intelligence Layer (from ConsultantCanvas)

The AI is not a chatbot. It is a **contextual intelligence engine** that permeates every surface.

#### 5.3.1 Context Awareness

The AI continuously ingests all workspace content — documents, diagrams, agent outputs, terminal sessions, git activity, conversations — and maintains a semantic index (RAG pipeline with vector embeddings). It knows what you're looking at and foregrounds relevant context.

#### 5.3.2 Proactive Intelligence

The AI doesn't wait to be asked:

- "Your architecture diagram shows a single database, but Agent-3 just created a PR that assumes a read replica. Should I flag this for review?"
- "The deployment agent failed because the staging environment doesn't have the IAM role your Terraform expects. Here's the fix."
- "You haven't updated the client portal in 5 days. Should I generate a progress summary from this week's agent activity?"

#### 5.3.3 Multi-Modal Understanding

- Reads and interprets Excalidraw diagrams
- Analyzes terminal session outputs
- Reviews code diffs and PR conversations
- Understands database schemas and data relationships
- Correlates information across all of these

#### 5.3.4 Deliverable Generation

Claude generates professional deliverables directly from workspace content:

- **Documents**: Findings reports, architecture docs, runbooks, proposals — formatted with branded templates, exported as .docx or PDF
- **Diagrams**: Mermaid diagrams from whiteboard sketches, architecture diagrams from code analysis
- **Runbooks**: Step-by-step operational guides generated from diagnostic sessions
- **Briefings**: Pre-meeting summaries of recent workspace activity

### 5.4 Guided Project Creation (BMAD-style Method)

Celerex integrates a structured, agent-guided project creation workflow inspired by the **BMAD Method** (Breakthrough Method for Agile AI-Driven Development). This is how consultants and teams go from "I have an idea" to "agents are building it" — with the client involved at every step.

#### 5.4.1 The Celerex Project Lifecycle

The project lifecycle is a guided, phased workflow where specialized AI agent personas walk the user (and optionally the client) through each stage. Each phase produces concrete artifacts that feed the next phase — and ultimately feed the coding agents.

```
Brainstorm → Analysis → PRD → UX Design → Architecture → Epics & Stories → Implementation → Delivery
```

| Phase | AI Agent Persona | Artifacts Produced | Client Involved? |
|-------|-----------------|-------------------|-----------------|
| **1. Brainstorm** | Facilitator | Product brief, problem statement, initial scope | Yes — collaborative ideation on canvas |
| **2. Research & Analysis** | Analyst | Market research, competitive analysis, technical feasibility, risk assessment | Optional — can review findings |
| **3. PRD Creation** | Product Manager | Full PRD via guided interview (not template-filling), requirements, success metrics, user personas | Yes — reviews and approves requirements |
| **4. UX Design** | UX Designer | Wireframes, user flows, interaction patterns (drawn on canvas) | Yes — collaborative design review |
| **5. Architecture** | Architect | System architecture doc, tech stack decisions, API design, infrastructure plan, architecture diagrams on canvas | Optional — reviews technical approach |
| **6. Epics & Stories** | Scrum Master / PM | Epic breakdown, user stories with acceptance criteria, story dependencies, sprint plan | Optional — reviews scope |
| **7. Implementation Readiness** | All agents (review) | Cross-check that PRD, UX, Architecture, and Stories are aligned. Gate before coding begins. | Informed |
| **8. Implementation** | Developer agents | Code, PRs, tests, deployments — orchestrated by the agentic layer (Section 5.2) | Sees progress in portal |
| **9. QA & Review** | QA agent | Test plans, E2E tests, code review, bug reports | Reviews deliverables |
| **10. Delivery & Handoff** | Tech Writer / PM | Documentation, runbooks, handoff package, client training materials | Receives deliverables |

#### 5.4.2 How It Works on the Canvas

Each phase plays out directly on the Celerex canvas:

- **The AI persona appears in the AI panel** — When you start the PRD phase, the AI adopts the Product Manager persona (personality, expertise, communication style). It interviews you, challenges your assumptions, and guides you through requirements discovery. It doesn't fill a template — it conducts a structured conversation and synthesizes the PRD from your answers.
- **Artifacts land on the canvas** — The PRD becomes a document block. Architecture diagrams appear on the edgeless canvas. User flows are drawn in the whiteboard. Epics become database/kanban blocks. Everything is spatially organized and cross-linked.
- **Client participates in real-time** — For collaborative phases (brainstorm, PRD review, UX design), the client joins the workspace and sees the AI-facilitated conversation. They can contribute, comment, and approve directly on the canvas.
- **Artifacts chain forward** — The PRD feeds the Architecture phase. Architecture feeds Epic creation. Epics feed the agent orchestrator. Each phase references and builds on prior artifacts, and the AI enforces consistency ("Your story says 'real-time sync' but the architecture doesn't include WebSockets — should we add it?").

#### 5.4.3 Agent Personas

Each AI persona has a distinct identity, expertise, and communication style (inspired by BMAD's named agents):

| Persona | Role | Style |
|---------|------|-------|
| **Product Manager** | PRD creation, requirements discovery, stakeholder alignment | Asks "why?" relentlessly. Cuts through fluff. User value first. |
| **Analyst** | Research, competitive analysis, feasibility assessment | Data-driven, thorough, presents options with trade-offs. |
| **Architect** | System design, tech stack, infrastructure planning | Calm, pragmatic. Balances "what could be" with "what should be." Favors boring technology that works. |
| **UX Designer** | Wireframes, user flows, interaction design | Visual thinker. Advocates for the user. Sketches on the canvas. |
| **Scrum Master** | Sprint planning, story creation, process facilitation | Organized, ensures nothing falls through cracks. Guards scope. |
| **Developer** | Implementation guidance, code review, technical debt assessment | Hands-on, practical. Knows what's easy and what's deceptively hard. |
| **QA Engineer** | Test strategy, E2E test generation, quality gates | Risk-based thinking. "What could go wrong?" Thorough but pragmatic. |
| **Tech Writer** | Documentation, runbooks, handoff materials | Clear, structured. Translates technical to accessible. |

These personas can operate individually (one phase at a time) or in **"Party Mode"** — multiple personas collaborating in a single session. For example, during Architecture, both the Architect and Developer personas might weigh in, with the Architect proposing the design and the Developer flagging implementation concerns.

#### 5.4.4 Implementation Readiness Gate

Before any code is written (by humans or agents), the system runs an **Implementation Readiness Check** — a cross-cutting review where the AI verifies:

- PRD requirements are reflected in the architecture
- Architecture decisions are captured in the epics/stories
- UX flows are covered by user stories
- Dependencies between stories are mapped
- No contradictions between artifacts

This gate prevents the costly pattern of "we started building before we were aligned." The check produces a readiness report with pass/fail items and suggested fixes.

#### 5.4.5 Customizable Workflows

The BMAD-style workflow is the default, but it's fully customizable:

- **Skip phases** — For a quick bug fix or small feature, skip straight to implementation
- **Add phases** — Insert a "Security Review" or "Cost Estimation" phase
- **Custom personas** — Define your own AI agent personas with specific expertise, tone, and domain knowledge
- **Custom artifact templates** — Define what a PRD, architecture doc, or epic looks like for your organization
- **Workflow templates** — Save and share complete workflow configurations ("Enterprise SaaS Project", "Quick Consulting Assessment", "Infrastructure Migration")

### 5.5 Engagement & Collaboration (from ConsultantCanvas)

#### 5.5.1 Engagement Management

For consulting and client-facing work, Celerex provides structured engagement lifecycle management:

- **Engagement lifecycle**: Configurable phases (Discovery → Assessment → Design → Implementation → Validation → Handoff) with gates and expected artifacts. The BMAD-style project lifecycle (Section 5.4) is the default workflow within an engagement.
- **Engagement templates**: Pre-built workspace configurations for common engagement types, including the full BMAD workflow pre-configured
- **Engagement memory**: The AI remembers everything — decisions, rationale, findings, context — across the full engagement timeline
- **Decision log**: Automatically captured decisions with context, alternatives considered, and supporting evidence

#### 5.5.2 Client / Stakeholder Portal

A permission-gated view for non-technical collaborators:

- **Artifact library**: Shared documents, diagrams, and deliverables in one organized view
- **Progress dashboard**: Visual timeline of phases, milestones, and work completed — auto-updated from workspace activity and agent progress
- **Evidence views**: Diagnostic findings and agent outputs presented in stakeholder-friendly formats
- **Comments & feedback**: Threaded comments on any shared artifact, indexed by the AI
- **Simplified AI**: Stakeholders get a Q&A AI that can explain technical findings at their level of understanding

#### 5.5.3 Collaborative Workspace Sharing

Beyond formal client portals, any workspace or page can be shared with configurable permissions:

| Permission | Access |
|-----------|--------|
| **Owner** | Full control — all blocks, agents, plugins, settings |
| **Editor** | Create and edit content, run agents with approval, use plugins |
| **Commenter** | View all shared content, leave comments, use AI Q&A |
| **Viewer** | Read-only access to shared content |

### 5.6 VS Code Integration

Celerex is a workspace for thinking and orchestrating. VS Code is where hands-on development happens. The two connect seamlessly — outputs from Celerex open in VS Code, and VS Code activity feeds back into the Celerex workspace context.

#### 5.6.1 Open in VS Code

Any code-oriented output in Celerex can be opened directly in VS Code with one click:

- **Agent output**: When an agent finishes a task, the resulting code/files can be opened in VS Code as a workspace. The PR diff, new files, and modified files all load in VS Code for hands-on review and editing.
- **Generated projects**: When the AI generates scaffolding, IaC configs, or boilerplate from a canvas brainstorm, the output directory opens in VS Code.
- **Terminal sessions**: A diagnostic terminal session in Celerex can be "promoted" to a full VS Code terminal with the same connection/context.
- **Code blocks**: Any code block on the canvas or in a document can be opened in VS Code with proper language mode, linting, and project context.
- **Repository workspaces**: Linked GitHub repos can be opened in VS Code (local clone) directly from the Celerex workspace nav.

#### 5.6.2 VS Code Extension

A companion **Celerex VS Code extension** provides:

- **Workspace sync**: The extension knows which Celerex workspace/engagement the current VS Code project belongs to. Changes in VS Code (file edits, git operations) are reflected in the Celerex activity feed.
- **Canvas sidebar**: A sidebar panel in VS Code showing the relevant Celerex canvas — architecture diagrams, task boards, agent status — without leaving the editor.
- **AI context bridge**: The Celerex AI has full context from both the workspace and the VS Code session. Ask the AI a question in VS Code and it knows about the architecture diagram you drew on the canvas.
- **Agent control**: Start, pause, review, and approve agent tasks from within VS Code. See agent PRs inline.
- **Push to canvas**: Select code, a terminal output, or a file tree in VS Code and push it as a block onto the Celerex canvas — for documentation, client sharing, or AI analysis.

#### 5.6.3 Protocol

Communication between Celerex and VS Code uses a local WebSocket connection (for desktop) or the VS Code Remote protocol (for cloud workspaces). The extension registers as an MCP (Model Context Protocol) server, allowing the Celerex AI to invoke VS Code operations (open file, run terminal command, show diff) as tools.

### 5.7 Plugin System (from ConsultantCanvas)

#### 5.7.1 Plugin Architecture

Plugins extend the platform with domain-specific capabilities. Each plugin declares:

- **Canvas blocks**: Custom block types that render in the workspace
- **AI tools**: Functions the AI can invoke when the plugin is active
- **AI prompts**: Domain-specific system prompts injected into AI context
- **Backend services**: Containerized processes for heavy workloads
- **Agent capabilities**: New skills and tools available to coding agents

#### 5.7.2 Scully-Mulder Diagnostic Framework

The diagnostic plugin pattern from ConsultantCanvas carries forward:

- **Scully (Evidence)**: Automated scanners, metric aggregation, compliance checks — structured, repeatable evidence collection
- **Mulder (Investigation)**: Interactive terminals, AI-assisted hypothesis testing, exploratory diagnostics

#### 5.7.3 Initial Plugin Catalog

| Plugin | Description |
|--------|-------------|
| **K8s Navigator** | Cluster diagnostics, pod management, AI-guided troubleshooting |
| **Cloud Ops (AWS/Azure/GCP)** | Resource inventory, cost analysis, security posture, IaC review |
| **CI/CD Inspector** | Pipeline health, build analysis, deployment history |
| **Data Platform** | Schema analysis, query optimization, data lineage |
| **ML Pipeline** | Experiment tracking, model drift detection, training cost analysis |
| **Infrastructure Planner** | Terraform/Pulumi workspace, drift detection, cost estimation |

---

## 6. Technical Architecture

### 6.1 Relationship to AFFiNE Codebase

AFFiNE is cloned as a **reference repository** — not forked, not used as a submodule. We study its architecture and selectively migrate code, patterns, and components into the Celerex codebase.

| AFFiNE Component | Celerex Approach |
|-----------------|-----------------|
| **BlockSuite** (`@blocksuite/*`) | Use as npm dependency where possible. BlockSuite is designed to be standalone and framework-agnostic. If customization needs exceed what BlockSuite supports, migrate relevant source into our tree. |
| **Edgeless/Page editors** (`@blocksuite/presets`) | Start with BlockSuite's PageEditor and EdgelessEditor presets. Extend with custom blocks (Agent Card, Terminal, etc.) using BlockSuite's block definition API. |
| **y-octo** (Rust CRDT) | Use directly in our Rust backend for server-side CRDT sync. This is AFFiNE's Rust implementation of Yjs — a natural fit. |
| **OctoBase** (Rust data engine) | Evaluate for adoption. If it fits our needs, use it. If too coupled to AFFiNE specifics, reimplement the local storage layer using y-octo + SQLite. |
| **AFFiNE app shell** (React, Electron) | Do NOT migrate. Build our own app shell on Bun + React with our own routing, layout, auth, and navigation. The AFFiNE app shell is too coupled to their product decisions. |
| **AFFiNE Cloud** (Node.js backend) | Do NOT migrate. Our backend is Rust (Axum). We build our own API, sync server, and services. |

### 6.2 Stack Overview

| Component | Technology | Rationale |
|-----------|-----------|-----------|
| **Frontend** | Bun + React + TypeScript | Bun for build tooling, dev server, and runtime. React for UI. Fast iteration, modern DX. |
| **Canvas/Editor** | BlockSuite (from AFFiNE) | Use BlockSuite packages directly. Extend with custom Celerex block types. Provides edgeless canvas, page editor, CRDT store, and inline editing out of the box. |
| **Backend API** | Rust (Axum + Tokio) | Core API, WebSocket management, CRDT sync server, agent orchestration. Replaces AFFiNE's Node.js backend. |
| **CRDT Sync** | y-octo (Rust, from AFFiNE) + Yjs (JS) | y-octo on the server, Yjs on the client (via BlockSuite). Native interop — y-octo is the Rust port of Yjs. |
| **Agent Orchestrator** | Rust | Manages agent lifecycle, task queues, context injection, conflict detection, GitHub integration. Performance-critical. |
| **AI Engine** | Rust + Python (FastAPI) | RAG pipeline, prompt orchestration, tool calling. Python where LLM ecosystem libraries are needed. |
| **Database** | PostgreSQL + pgvector | Relational data + vector embeddings for semantic search. |
| **Local Storage** | SQLite + y-octo CRDT log | Local-first data layer. Syncs to cloud via CRDT merge. Mirrors AFFiNE's OctoBase approach. |
| **Object Storage** | S3-compatible (R2/S3) | Documents, images, exports, agent artifacts, plugin assets. |
| **Auth** | Custom or Clerk | Multi-tenant auth, org support, SSO, invitation flows. |
| **Plugin Sandbox** | WebAssembly (Wasmtime) + containers | Lightweight plugins as WASM. Heavy plugins in isolated containers. |
| **Agent Runtime** | Container-based | Each agent runs in an isolated container with scoped filesystem, credentials, and network access. |
| **Source Control** | GitHub API + GitHub Apps | Deep integration for repos, PRs, issues, Actions, and multi-repo orchestration. |
| **VS Code Integration** | VS Code Extension + WebSocket/MCP | Companion extension bridges Celerex workspace with VS Code. Local WebSocket for desktop, MCP protocol for AI tool invocation. |
| **Desktop** | Electron (or Tauri) | Cross-platform desktop app. AFFiNE uses Electron; we may evaluate Tauri (Rust-native) for tighter backend integration. |

### 6.3 Architecture Layers

```
+---------------------------------------------------------------+
|                   Frontend (Bun + React)                       |
|  BlockSuite (from AFFiNE)  |  Celerex App Shell               |
|  PageEditor | EdgelessEditor | Custom Blocks (Agent, Terminal) |
|  Agent UI | Plugin UI | Stakeholder Portal                    |
+---------------------------------------------------------------+
        |              |              |               |
        v              v              v               v
+---------------------------------------------------------------+
|                   Rust Backend (Axum + Tokio)                  |
| API | y-octo CRDT Sync | Auth | Permissions | Search          |
+---------------------------------------------------------------+
        |              |              |               |
        v              v              v               v
+-------------+ +---------------+ +--------------+ +------------+
| Agent       | | AI Engine     | | Plugin       | | VS Code    |
| Orchestrator| | (RAG+Prompts) | | Runtime      | | Bridge     |
| (Rust)      | | (Rust+Python) | | (WASM+Ctnr) | | (WS+MCP)  |
+-------------+ +---------------+ +--------------+ +------------+
        |              |              |               |
        v              v              v               v
+---------------------------------------------------------------+
| PostgreSQL+pgvector | S3 Storage | GitHub API | Vault          |
+---------------------------------------------------------------+
```

### 6.3 Local-First Architecture

Following AFFiNE's approach:

1. **All writes go to local CRDT first** — instant responsiveness, full offline support
2. **Background sync** merges local CRDTs with cloud state — conflict-free by design
3. **Cloud is the collaboration hub** — real-time presence, cursor sharing, and sync happen through the Rust WebSocket server
4. **Data export anytime** — workspace data can be exported as Markdown, JSON, or SQLite at any point

---

## 7. Phased Delivery Roadmap

| Phase | Deliverables | Duration |
|-------|-------------|----------|
| **Phase 1: Canvas Foundation** | Clone AFFiNE as reference. Stand up Celerex app shell (Bun + React). Integrate BlockSuite (PageEditor + EdgelessEditor). Implement Rust backend with y-octo CRDT sync. Auth + multi-tenant. Basic AI chat panel. VS Code "open in" integration (one-way). | 12–14 weeks |
| **Phase 2: Agentic Core** | Agent orchestrator (Rust). Single-agent task execution via GitHub. Agent Card block type on canvas. PR dashboard. Context injection from workspace content. Human-in-the-loop gates. VS Code extension (basic sidebar + sync). | 10–12 weeks |
| **Phase 3: Multi-Agent & Collaboration** | Multi-agent orchestration strategies (fan-out, pipeline, swarm). Real-time collaboration (multi-user sync). Stakeholder portal. Engagement lifecycle management. Deliverable generation (.docx, PDF). Full VS Code bidirectional bridge. | 12–14 weeks |
| **Phase 4: Plugin Ecosystem** | Plugin runtime (WASM + containers). Scully-Mulder diagnostic framework. First plugins (K8s, Cloud Ops). Plugin marketplace. Cross-workspace knowledge graph. Desktop app (Electron or Tauri). | 12–16 weeks |

---

## 8. How Celerex Differs

| | Notion/Confluence | AFFiNE | GitHub/Linear | Devin/Cursor | **Celerex** |
|---|---|---|---|---|---|
| Knowledge canvas | Yes | Yes | No | No | **Yes (AFFiNE-based)** |
| Spatial/edgeless workspace | No | Yes | No | No | **Yes (AFFiNE-based)** |
| Local-first | No | Yes | No | No | **Yes (AFFiNE-based)** |
| AI coding agents | No | No | Limited | Yes (single) | **Multi-agent orchestration** |
| GitHub deep integration | No | No | Yes | Limited | **Yes** |
| VS Code integration | No | No | No | Yes (built-in) | **Extension + bridge** |
| Diagnostic tooling | No | No | No | No | **Plugin-based** |
| Client/stakeholder portal | No | No | No | No | **Yes** |
| Engagement lifecycle | No | No | No | No | **Yes** |
| Professional deliverables | No | No | No | No | **AI-generated .docx/PDF** |
| Plugin ecosystem | Limited | Coming soon | GitHub Apps | No | **Full plugin runtime** |

---

## 9. Success Metrics

| Metric | Target |
|--------|--------|
| Agent task completion rate (without human intervention) | > 60% |
| Tool consolidation (tools replaced per team) | 4+ tools replaced |
| Time from idea to deployed code (agent-assisted) | 50% reduction |
| Workspace adoption (daily active usage) | > 80% of team members |
| Client/stakeholder satisfaction (NPS) | > 50 |
| Plugin ecosystem (available plugins within 12 months) | 10+ plugins |

---

## 10. Open Questions

- **BlockSuite as dependency vs. migration**: Use BlockSuite npm packages directly (faster, get updates) or migrate source into our tree (more control, can diverge)? Start with packages, migrate if needed?
- **Agent provider strategy**: Support multiple agent backends (Claude, OpenAI, open-source) or deep-integrate with one? Start with Claude, add others later?
- **Local-first vs. cloud-first for agents**: Agents need cloud compute, but workspace is local-first. How to reconcile? Likely: workspace is local-first, agent orchestration is cloud-native.
- **Electron vs. Tauri for desktop**: AFFiNE uses Electron. Tauri is Rust-native and lighter, which fits our Rust backend. But Electron has broader ecosystem support and BlockSuite is tested against it.
- **VS Code extension scope**: Minimal (just "open in VS Code" + sidebar) vs. full bidirectional bridge? Start minimal, expand based on usage?
- **Self-hosted deployment**: Priority for air-gapped / enterprise environments? Adds complexity but expands addressable market significantly.
- **Pricing model**: Per-seat? Per-agent-minute? Hybrid? Agent compute costs are real and variable.
- **Agent security model**: How much filesystem/network access do agents get? Per-task scoping? Credential injection?
- **AFFiNE version tracking**: Which AFFiNE release do we clone as reference? How do we track upstream improvements worth migrating?

---

## 11. Name

**Celerex** — from Latin *celer* (swift, fast) + *rex* (king/ruler) — connotes speed and command. A platform that lets you move fast and stay in control.

---

*This is a living document. Next steps: validate architecture assumptions with prototyping, prioritize Phase 1 feature set, and begin technical spikes on CRDT engine and agent orchestrator.*
