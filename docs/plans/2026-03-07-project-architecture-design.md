# Celerex Project Architecture Restructuring — Design Document

**Date:** 2026-03-07
**Status:** Approved

## Overview

Restructure Celerex from a flat Engagement → Documents model to a hierarchical Engagement → Project → Documents model. Projects become the primary unit of work, containing documents, assigned agents, pipelines, triggers, and jobs. Add backend persistence with PostgreSQL for "start anywhere" portability and Kubernetes-ready multi-pod scaling. Integrate a BMAD-style Solution Builder wizard for guided project setup. Add real-time execution visualization views.

## Data Model

### Hierarchy

```
Engagement (organizational container — client/contract boundary)
  └── Project (unit of work — all content lives here)
        ├── Documents (BlockSuite docs)
        ├── ProjectAgents (assigned from global catalog, with overrides)
        ├── Pipelines (workflows referencing ProjectAgents)
        ├── TriggerBindings (project-scoped automation)
        └── Jobs (execution history)

Global Catalog (portable, reusable templates):
  ├── AgentDef[] (built-in + custom agent templates)
  ├── ToolDef[] (built-in + custom tool definitions)
  └── TriggerDef[] (reusable trigger definitions)
```

### Core Types

```typescript
interface Engagement {
  id: string
  name: string
  createdAt: number
}

interface Project {
  id: string
  name: string
  code: string              // Project Code (future: WBSE)
  engagementId: string
  createdAt: number
}

interface DocMeta {
  id: string
  title: string
  projectId: string         // changed from engagementId
  createdAt: number
}

interface ProjectAgent {
  id: string
  agentDefId: string        // references global AgentDef
  projectId: string
  systemPrompt?: string     // override template's prompt
  model?: string            // override template's model
  tools?: string[]          // override/restrict available tools
  config?: Record<string, any>
}

interface Pipeline {
  id: string
  name: string
  projectId: string
  nodes: PipelineNode[]     // nodes reference ProjectAgent.id
  edges: PipelineEdge[]
}

interface TriggerBinding {
  id: string
  triggerDefId: string
  projectId: string
  agentId: string           // ProjectAgent ID
  config?: Record<string, any>
  enabled: boolean
}
```

### Agent Model

Agent definitions (AgentDef) are global templates — they define what an agent can do. ProjectAgent is an assignment instance — it says "this project uses this agent, with these overrides."

```
AgentDef "Code Reviewer" (global template)
  │
  ├── ProjectAgent in Project A: uses Claude, strict rules
  ├── ProjectAgent in Project B: uses GPT-4o, relaxed rules
  └── Project C: doesn't use this agent at all
```

Pipeline nodes reference ProjectAgent IDs, not AgentDef IDs. The Workflow Designer only shows agents assigned to the current project.

## Persistence

### Backend-First with SQLite

All data persists through the Rust API backed by SQLite. IndexedDB serves as a local cache for speed/offline.

```
Frontend (React/Jotai) ──REST──► celerex-api (port 3001) ──► PostgreSQL
       │                                                       (Docker for dev,
  IndexedDB (local cache)                                       managed for prod)
```

**Why Postgres over SQLite:**
- K8s-ready: multiple API pods connect to one Postgres instance
- Concurrent writes from multiple pods without data divergence
- Native JSON/JSONB support for structured fields
- Production-grade with connection pooling via sqlx

**Dev setup:** `docker compose up -d postgres` — zero-config local Postgres.

### Database Schema

```sql
-- Core hierarchy
CREATE TABLE engagements (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE projects (
  id TEXT PRIMARY KEY,
  engagement_id TEXT NOT NULL REFERENCES engagements(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  code TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE documents (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Global catalog
CREATE TABLE agent_defs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
  definition JSONB NOT NULL
);

CREATE TABLE tool_defs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  category TEXT NOT NULL,
  is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
  definition JSONB NOT NULL
);

CREATE TABLE trigger_defs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  is_builtin BOOLEAN NOT NULL DEFAULT FALSE,
  definition JSONB NOT NULL
);

-- Project-scoped assignments
CREATE TABLE project_agents (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  agent_def_id TEXT NOT NULL REFERENCES agent_defs(id),
  overrides JSONB
);

CREATE TABLE pipelines (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  nodes JSONB NOT NULL DEFAULT '[]',
  edges JSONB NOT NULL DEFAULT '[]'
);

CREATE TABLE trigger_bindings (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  trigger_def_id TEXT NOT NULL REFERENCES trigger_defs(id),
  agent_id TEXT NOT NULL REFERENCES project_agents(id),
  config JSONB,
  enabled BOOLEAN NOT NULL DEFAULT TRUE
);

CREATE TABLE jobs (
  id TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  pipeline_id TEXT REFERENCES pipelines(id),
  status TEXT NOT NULL,
  result JSONB,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMPTZ
);

-- Settings
CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value JSONB NOT NULL
);
```

### API Surface

```
# Engagements
GET    /api/engagements
POST   /api/engagements
PUT    /api/engagements/{id}
DELETE /api/engagements/{id}

# Projects
GET    /api/engagements/{id}/projects
POST   /api/engagements/{id}/projects
PUT    /api/projects/{id}
DELETE /api/projects/{id}
GET    /api/projects/{id}                    # full project with docs, agents, pipelines

# Documents (scoped to project)
GET    /api/projects/{id}/docs
POST   /api/projects/{id}/docs
PUT    /api/docs/{id}
DELETE /api/docs/{id}

# Project Agents
GET    /api/projects/{id}/agents
POST   /api/projects/{id}/agents
PUT    /api/project-agents/{id}
DELETE /api/project-agents/{id}

# Pipelines
GET    /api/projects/{id}/pipelines
POST   /api/projects/{id}/pipelines
PUT    /api/pipelines/{id}
DELETE /api/pipelines/{id}

# Global Catalog
GET    /api/agent-defs
POST   /api/agent-defs                       # custom only
PUT    /api/agent-defs/{id}                  # custom only
DELETE /api/agent-defs/{id}                  # custom only

GET    /api/tool-defs
POST   /api/tool-defs
PUT    /api/tool-defs/{id}
DELETE /api/tool-defs/{id}

GET    /api/trigger-defs
POST   /api/trigger-defs
PUT    /api/trigger-defs/{id}
DELETE /api/trigger-defs/{id}

# Settings
GET    /api/settings
PUT    /api/settings

# Existing: chat, copilot auth, execution agents/jobs
```

## UI Changes

### Sidebar

```
Sidebar
├── Engagement: "Acme Corp"
│   ├── Project: "Cloud Migration" (ACM-001)  ← click enters project context
│   │   ├── 📄 Discovery Brief
│   │   ├── 📄 Architecture Doc
│   │   └── 📄 Migration Runbook
│   └── Project: "Data Platform" (ACM-002)
│       └── 📄 Data Pipeline Spec
└── Engagement: "Globex Inc"
    └── Project: "Portal Redesign" (GLX-001)
        └── 📄 ...
```

### Views

| View | Purpose | Scope |
|---|---|---|
| Editor | Document editing (BlockSuite) | Project doc |
| Agent Catalog | Browse/create global agent templates | Global |
| Project Agents | Agents assigned to this project, configure overrides | Project |
| Workflow Designer | Build pipelines (was Pipeline Editor) | Project |
| Workflow Monitor | Live execution view of running pipeline | Project |
| Agent Runner | TUI-style terminal view of agent output streams | Project |
| Task Queue | Job list with status, logs, retries | Project |
| Solution Builder | BMAD wizard for guided project setup | Project |
| Tools Manager | Global tool catalog + custom tools | Global |
| Triggers | Trigger definitions + project bindings | Both |
| Settings | App preferences | Global |

### View Renames

- "Pipeline Editor" → **Workflow Designer** (design mode) / **Workflow Monitor** (run mode)
- "Agent Dashboard" → **Agent Catalog** (global) + **Project Agents** (project-scoped)

## BMAD Solution Builder

### User Flow

1. User creates Engagement, then creates a Project within it
2. Inside the project, user clicks "Start Solution" to launch the Solution Builder
3. Solution Builder presents BMAD phases and asks which to include
4. Guided Q&A walks through each selected phase, generating artifacts
5. Artifacts become documents in the project
6. System suggests agents to assign and pipelines to create
7. User customizes and runs

### BMAD Phases (from PRD section 5.4)

| Phase | AI Persona | Artifacts |
|---|---|---|
| 1. Brainstorm | Facilitator | Product brief, problem statement, scope |
| 2. Research & Analysis | Analyst | Market research, competitive analysis, feasibility |
| 3. PRD Creation | Product Manager | PRD, requirements, success metrics, personas |
| 4. UX Design | UX Designer | Wireframes, user flows, interaction patterns |
| 5. Architecture | Architect | System architecture, tech stack, API design |
| 6. Epics & Stories | Scrum Master | Epic breakdown, user stories, sprint plan |
| 7. Implementation Readiness | All (review) | Cross-check alignment |
| 8. Implementation | Developer | Code, PRs, tests, deployments |
| 9. QA & Review | QA | Test plans, E2E tests, bug reports |
| 10. Delivery & Handoff | Tech Writer | Documentation, runbooks, training |

### Implementation

The Solution Builder is a state-machine-driven wizard UI in the frontend. Each phase:
1. Presents context from previous phases
2. Uses the chat API (LLM) to ask BMAD-style questions
3. Collects answers and generates structured artifacts
4. Saves artifacts as project documents
5. Advances to next phase

## Real-Time Execution Views

### Workflow Monitor

The Workflow Designer in "live" mode:
- Same node graph rendered read-only during execution
- Nodes show real-time status via color: gray=pending, blue=running, green=complete, red=failed
- Edges animate to show data flowing between agents
- Click a node to see its output stream in a panel
- Progress percentage on each node

### Agent Runner

VS Code / tmux-style terminal view:
- Split panes showing active agent stdout/stderr
- SSE streaming from orchestrator → API → frontend
- Auto-scroll with pause button
- Multiple agents visible simultaneously
- Scrollback buffer with search

### Task Queue (Enhanced)

- Filterable by project, pipeline, agent, status
- Job detail panel with full logs
- Retry/cancel controls
- Duration and timing information
