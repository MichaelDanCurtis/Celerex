interface Task {
  id: string;
  name: string;
  agent: string;
  status: "Running" | "Pending" | "Completed";
  timestamp: string;
  progress?: number;
}

const tasks: Task[] = [
  {
    id: "t1",
    name: "Summarizing Project Overview",
    agent: "Document Assistant",
    status: "Running",
    timestamp: "Started 2 min ago",
    progress: 65,
  },
  {
    id: "t2",
    name: "Lint Check on utils/export.ts",
    agent: "Code Reviewer",
    status: "Pending",
    timestamp: "Queued 5 min ago",
  },
  {
    id: "t3",
    name: "Convert CSV to JSON (dataset_q4)",
    agent: "Data Formatter",
    status: "Pending",
    timestamp: "Queued 8 min ago",
  },
  {
    id: "t4",
    name: "Research: AI governance frameworks",
    agent: "Research Agent",
    status: "Completed",
    timestamp: "Completed 12 min ago",
  },
  {
    id: "t5",
    name: "Draft executive summary",
    agent: "Document Assistant",
    status: "Completed",
    timestamp: "Completed 25 min ago",
  },
  {
    id: "t6",
    name: "Format quarterly report data",
    agent: "Data Formatter",
    status: "Completed",
    timestamp: "Completed 1 hr ago",
  },
];

const statusColors: Record<Task["status"], string> = {
  Running: "var(--celerex-accent)",
  Pending: "#d97706",
  Completed: "#22c55e",
};

export function TaskQueue() {
  const running = tasks.filter((t) => t.status === "Running");
  const pending = tasks.filter((t) => t.status === "Pending");
  const completed = tasks.filter((t) => t.status === "Completed");

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
          Task Queue
        </h1>
        <p
          style={{
            margin: "4px 0 24px",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          Track agent task progress
        </p>

        {/* Running */}
        <Section title="Running" count={running.length}>
          {running.map((task) => (
            <TaskCard key={task.id} task={task} />
          ))}
        </Section>

        {/* Pending */}
        <Section title="Pending" count={pending.length}>
          {pending.map((task) => (
            <TaskCard key={task.id} task={task} />
          ))}
        </Section>

        {/* Completed */}
        <Section
          title="Completed"
          count={completed.length}
          action={
            <button
              onClick={() => {}}
              style={{
                padding: "5px 14px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "transparent",
                color: "var(--celerex-text-secondary)",
                fontSize: 12,
                fontWeight: 500,
                cursor: "pointer",
              }}
            >
              Clear Completed
            </button>
          }
        >
          {completed.map((task) => (
            <TaskCard key={task.id} task={task} />
          ))}
        </Section>
      </div>
    </div>
  );
}

function Section({
  title,
  count,
  action,
  children,
}: {
  title: string;
  count: number;
  action?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 28 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: 12,
        }}
      >
        <h2
          style={{
            margin: 0,
            fontSize: 16,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          {title}{" "}
          <span
            style={{
              fontSize: 13,
              fontWeight: 400,
              color: "var(--celerex-text-secondary)",
            }}
          >
            ({count})
          </span>
        </h2>
        {action}
      </div>
      <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
        {children}
      </div>
    </div>
  );
}

function TaskCard({ task }: { task: Task }) {
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
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          {task.status === "Completed" && (
            <span style={{ color: "#22c55e", fontSize: 15, lineHeight: 1 }}>
              &#10003;
            </span>
          )}
          <span
            style={{
              fontSize: 15,
              fontWeight: 600,
              color: "var(--celerex-text-primary)",
            }}
          >
            {task.name}
          </span>
        </div>
        <span
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 5,
            fontSize: 12,
            color: statusColors[task.status],
          }}
        >
          <span
            style={{
              width: 7,
              height: 7,
              borderRadius: "50%",
              background: statusColors[task.status],
            }}
          />
          {task.status === "Pending" ? "Queued" : task.status}
        </span>
      </div>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <span
          style={{
            fontSize: 13,
            color: "var(--celerex-text-secondary)",
          }}
        >
          {task.agent}
        </span>
        <span
          style={{
            fontSize: 12,
            color: "var(--celerex-text-secondary)",
          }}
        >
          {task.timestamp}
        </span>
      </div>

      {task.status === "Running" && task.progress != null && (
        <div
          style={{
            marginTop: 4,
            height: 6,
            borderRadius: 3,
            background: "var(--celerex-border)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              width: `${task.progress}%`,
              height: "100%",
              borderRadius: 3,
              background: "var(--celerex-accent)",
              transition: "width 0.3s ease",
            }}
          />
        </div>
      )}
    </div>
  );
}
