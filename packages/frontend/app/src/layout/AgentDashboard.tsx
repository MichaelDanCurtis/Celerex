import { useState, useMemo } from "react";
import {
  CATEGORIES,
  searchAgents,
  type AgentDef,
  type CategoryId,
} from "../data/agents";

const statusColors: Record<AgentDef["status"], string> = {
  active: "#22c55e",
  idle: "#9ca3af",
  disabled: "#ef4444",
};

const statusLabels: Record<AgentDef["status"], string> = {
  active: "Active",
  idle: "Idle",
  disabled: "Disabled",
};

const PAGE_SIZE = 50;

export function AgentDashboard() {
  const [activeCategory, setActiveCategory] = useState<CategoryId>("document-management");
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<AgentDef["status"] | "all">("all");
  const [page, setPage] = useState(0);
  const [selectedAgent, setSelectedAgent] = useState<AgentDef | null>(null);

  const filtered = useMemo(() => {
    let results = searchAgents(search, activeCategory);
    if (statusFilter !== "all") {
      results = results.filter((a) => a.status === statusFilter);
    }
    return results;
  }, [search, activeCategory, statusFilter]);

  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);
  const paged = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);

  const counts = useMemo(() => {
    const all = searchAgents("", activeCategory);
    return {
      total: all.length,
      active: all.filter((a) => a.status === "active").length,
      idle: all.filter((a) => a.status === "idle").length,
      disabled: all.filter((a) => a.status === "disabled").length,
    };
  }, [activeCategory]);

  // Reset page when filters change
  const handleCategoryChange = (id: CategoryId) => {
    setActiveCategory(id);
    setPage(0);
    setSelectedAgent(null);
  };
  const handleSearchChange = (v: string) => {
    setSearch(v);
    setPage(0);
  };
  const handleStatusChange = (v: AgentDef["status"] | "all") => {
    setStatusFilter(v);
    setPage(0);
  };

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "auto",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 1100, margin: "0 auto", padding: "32px 24px" }}>
        {/* Header */}
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
            margin: "4px 0 20px",
            fontSize: 14,
            color: "var(--celerex-text-secondary)",
          }}
        >
          Browse and manage {CATEGORIES.length} agent categories
        </p>

        {/* Category Tabs */}
        <div
          style={{
            display: "flex",
            gap: 0,
            borderBottom: "2px solid var(--celerex-border)",
            marginBottom: 20,
          }}
        >
          {CATEGORIES.map((cat) => {
            const isActive = activeCategory === cat.id;
            return (
              <button
                key={cat.id}
                onClick={() => handleCategoryChange(cat.id)}
                style={{
                  padding: "10px 20px",
                  fontSize: 13,
                  fontWeight: isActive ? 600 : 400,
                  color: isActive
                    ? "var(--celerex-accent)"
                    : "var(--celerex-text-secondary)",
                  background: "transparent",
                  border: "none",
                  borderBottom: isActive
                    ? "2px solid var(--celerex-accent)"
                    : "2px solid transparent",
                  marginBottom: -2,
                  cursor: "pointer",
                  transition: "all 0.15s",
                  whiteSpace: "nowrap",
                }}
              >
                {cat.label}
              </button>
            );
          })}
        </div>

        {/* Category description + stats */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            marginBottom: 16,
            flexWrap: "wrap",
            gap: 12,
          }}
        >
          <p
            style={{
              margin: 0,
              fontSize: 13,
              color: "var(--celerex-text-secondary)",
            }}
          >
            {CATEGORIES.find((c) => c.id === activeCategory)?.description}
          </p>
          <div style={{ display: "flex", gap: 12, fontSize: 12 }}>
            <StatBadge label="Total" value={counts.total} color="var(--celerex-text-primary)" />
            <StatBadge label="Active" value={counts.active} color="#22c55e" />
            <StatBadge label="Idle" value={counts.idle} color="#9ca3af" />
            <StatBadge label="Disabled" value={counts.disabled} color="#ef4444" />
          </div>
        </div>

        {/* Search + Filter Bar */}
        <div
          style={{
            display: "flex",
            gap: 8,
            marginBottom: 16,
            flexWrap: "wrap",
          }}
        >
          <input
            type="text"
            value={search}
            onChange={(e) => handleSearchChange(e.target.value)}
            placeholder="Search agents..."
            style={{
              flex: 1,
              minWidth: 200,
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              fontSize: 13,
              outline: "none",
            }}
          />
          <select
            value={statusFilter}
            onChange={(e) => handleStatusChange(e.target.value as any)}
            style={{
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              fontSize: 13,
              outline: "none",
              cursor: "pointer",
            }}
          >
            <option value="all">All Status</option>
            <option value="active">Active</option>
            <option value="idle">Idle</option>
            <option value="disabled">Disabled</option>
          </select>
        </div>

        {/* Results count */}
        <div
          style={{
            fontSize: 12,
            color: "var(--celerex-text-secondary)",
            marginBottom: 12,
          }}
        >
          Showing {paged.length} of {filtered.length} agents
          {search && ` matching "${search}"`}
          {totalPages > 1 && ` (page ${page + 1} of ${totalPages})`}
        </div>

        {/* Agent Grid */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
            gap: 12,
          }}
        >
          {paged.map((agent) => (
            <AgentCard
              key={agent.id}
              agent={agent}
              selected={selectedAgent?.id === agent.id}
              onSelect={() =>
                setSelectedAgent(selectedAgent?.id === agent.id ? null : agent)
              }
            />
          ))}
        </div>

        {paged.length === 0 && (
          <div
            style={{
              padding: "40px 16px",
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              fontSize: 14,
            }}
          >
            No agents found{search ? ` matching "${search}"` : ""}
          </div>
        )}

        {/* Pagination */}
        {totalPages > 1 && (
          <div
            style={{
              display: "flex",
              justifyContent: "center",
              gap: 8,
              marginTop: 20,
            }}
          >
            <PageButton
              label="Prev"
              disabled={page === 0}
              onClick={() => setPage((p) => p - 1)}
            />
            {Array.from({ length: totalPages }, (_, i) => (
              <PageButton
                key={i}
                label={String(i + 1)}
                active={i === page}
                onClick={() => setPage(i)}
              />
            ))}
            <PageButton
              label="Next"
              disabled={page >= totalPages - 1}
              onClick={() => setPage((p) => p + 1)}
            />
          </div>
        )}

        {/* Selected Agent Detail */}
        {selectedAgent && (
          <div
            style={{
              marginTop: 24,
              padding: 20,
              borderRadius: 10,
              border: "1px solid var(--celerex-accent)",
              background: "var(--celerex-bg-sidebar)",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: 12,
              }}
            >
              <h3
                style={{
                  margin: 0,
                  fontSize: 16,
                  fontWeight: 600,
                  color: "var(--celerex-text-primary)",
                }}
              >
                {selectedAgent.name}
              </h3>
              <StatusPill status={selectedAgent.status} />
            </div>
            <p
              style={{
                margin: "0 0 12px",
                fontSize: 13,
                lineHeight: 1.5,
                color: "var(--celerex-text-secondary)",
              }}
            >
              {selectedAgent.description}
            </p>
            <div
              style={{
                display: "flex",
                gap: 8,
                fontSize: 12,
                color: "var(--celerex-text-secondary)",
                marginBottom: 16,
              }}
            >
              <span>ID: <code style={{ fontFamily: "monospace", fontSize: 11 }}>{selectedAgent.id}</code></span>
              <span>Category: {CATEGORIES.find((c) => c.id === selectedAgent.category)?.label}</span>
            </div>
            <div style={{ display: "flex", gap: 8 }}>
              <ActionButton label="Configure" />
              <ActionButton label="View Logs" />
              {selectedAgent.status === "disabled" ? (
                <ActionButton label="Enable" accent />
              ) : (
                <ActionButton label="Disable" danger />
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function StatBadge({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        color,
        fontWeight: 500,
      }}
    >
      {value} {label}
    </span>
  );
}

function StatusPill({ status }: { status: AgentDef["status"] }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 5,
        fontSize: 12,
        color: statusColors[status],
        fontWeight: 500,
      }}
    >
      <span
        style={{
          width: 7,
          height: 7,
          borderRadius: "50%",
          background: statusColors[status],
        }}
      />
      {statusLabels[status]}
    </span>
  );
}

function AgentCard({
  agent,
  selected,
  onSelect,
}: {
  agent: AgentDef;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <div
      onClick={onSelect}
      style={{
        padding: "12px 14px",
        borderRadius: 8,
        border: selected
          ? "1px solid var(--celerex-accent)"
          : "1px solid var(--celerex-border)",
        background: selected
          ? "var(--celerex-bg-secondary)"
          : "var(--celerex-bg-sidebar)",
        cursor: "pointer",
        transition: "border-color 0.15s, background 0.15s",
        display: "flex",
        flexDirection: "column",
        gap: 6,
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
            fontSize: 13,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {agent.name}
        </span>
        <StatusPill status={agent.status} />
      </div>
      <p
        style={{
          margin: 0,
          fontSize: 12,
          lineHeight: 1.4,
          color: "var(--celerex-text-secondary)",
          overflow: "hidden",
          textOverflow: "ellipsis",
          display: "-webkit-box",
          WebkitLineClamp: 2,
          WebkitBoxOrient: "vertical",
        }}
      >
        {agent.description}
      </p>
    </div>
  );
}

function PageButton({
  label,
  active,
  disabled,
  onClick,
}: {
  label: string;
  active?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        padding: "4px 10px",
        borderRadius: 4,
        border: "1px solid var(--celerex-border)",
        background: active ? "var(--celerex-accent)" : "var(--celerex-bg-secondary)",
        color: active ? "#fff" : disabled ? "var(--celerex-text-secondary)" : "var(--celerex-text-primary)",
        cursor: disabled ? "not-allowed" : "pointer",
        fontSize: 12,
        fontWeight: active ? 600 : 400,
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {label}
    </button>
  );
}

function ActionButton({
  label,
  accent,
  danger,
}: {
  label: string;
  accent?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      onClick={() => {}}
      style={{
        padding: "6px 16px",
        borderRadius: 6,
        border: accent || danger ? "none" : "1px solid var(--celerex-border)",
        background: accent
          ? "var(--celerex-accent)"
          : danger
          ? "#ef4444"
          : "transparent",
        color: accent || danger ? "#fff" : "var(--celerex-text-primary)",
        fontSize: 12,
        fontWeight: 500,
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}
