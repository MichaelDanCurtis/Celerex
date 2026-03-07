interface AgentInfo {
  name: string;
  status: "Active" | "Idle" | "Disabled";
  description: string;
}

const agents: AgentInfo[] = [
  {
    name: "Document Assistant",
    status: "Active",
    description:
      "Helps draft, summarize, and refine documents within your workspace.",
  },
  {
    name: "Code Reviewer",
    status: "Idle",
    description:
      "Analyzes code snippets for best practices, bugs, and style improvements.",
  },
  {
    name: "Research Agent",
    status: "Active",
    description:
      "Gathers and synthesizes information from multiple sources on a given topic.",
  },
  {
    name: "Data Formatter",
    status: "Disabled",
    description:
      "Converts and cleans structured data between CSV, JSON, and table formats.",
  },
];

const statusColors: Record<AgentInfo["status"], string> = {
  Active: "#22c55e",
  Idle: "#9ca3af",
  Disabled: "#ef4444",
};

export function AgentDashboard() {
  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "auto",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 800, margin: "0 auto", padding: "32px 24px" }}>
        <h1
          style={{
            margin: 0,
            fontSize: 22,
            fontWeight: 700,
            color: "var(--celerex-text-primary)",
          }}
        >
          Agent Dashboard
        </h1>
        <p
          style={{
            margin: "4px 0 24px",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          Manage and monitor your AI agents
        </p>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(340px, 1fr))",
            gap: 16,
          }}
        >
          {agents.map((agent) => (
            <AgentCard key={agent.name} agent={agent} />
          ))}
        </div>

        <div style={{ marginTop: 32 }}>
          <h2
            style={{
              margin: "0 0 12px",
              fontSize: 16,
              fontWeight: 600,
              color: "var(--celerex-text-primary)",
            }}
          >
            Running Tasks
          </h2>
          <div
            style={{
              padding: "20px 16px",
              borderRadius: 8,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-sidebar)",
              color: "var(--celerex-text-secondary)",
              fontSize: 13,
              textAlign: "center",
            }}
          >
            No tasks currently running
          </div>
        </div>
      </div>
    </div>
  );
}

function AgentCard({ agent }: { agent: AgentInfo }) {
  return (
    <div
      style={{
        padding: 16,
        borderRadius: 8,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: 15,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          {agent.name}
        </span>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            fontSize: 12,
            color: statusColors[agent.status],
          }}
        >
          <span
            style={{
              width: 7,
              height: 7,
              borderRadius: "50%",
              background: statusColors[agent.status],
            }}
          />
          {agent.status}
        </span>
      </div>
      <p
        style={{
          margin: 0,
          fontSize: 13,
          lineHeight: 1.45,
          color: "var(--celerex-text-secondary)",
        }}
      >
        {agent.description}
      </p>
      <div style={{ marginTop: 4 }}>
        <button
          onClick={() => {}}
          style={{
            padding: "5px 14px",
            borderRadius: 6,
            border: "1px solid var(--celerex-border)",
            background: "transparent",
            color: "var(--celerex-text-primary)",
            fontSize: 12,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Configure
        </button>
      </div>
    </div>
  );
}
