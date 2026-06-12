import { useState, useMemo } from "react";
import {
  getAllTriggers,
  addTrigger,
  updateTrigger,
  deleteTrigger,
  toggleTrigger,
  TRIGGER_TYPES,
  type TriggerDef,
  type TriggerType,
  type TriggerConfig,
  type EventConfig,
  type QueueConfig,
} from "../data/triggers";
import { getAllAgents } from "../data/agents";

/* ------------------------------------------------------------------ */
/*  Helpers                                                           */
/* ------------------------------------------------------------------ */

function getTriggerTypeMeta(type: TriggerType) {
  return TRIGGER_TYPES.find((t) => t.id === type) ?? TRIGGER_TYPES[0];
}

function formatLastRun(iso?: string): string {
  if (!iso) return "Never";
  const d = new Date(iso);
  const now = new Date();
  const diff = now.getTime() - d.getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "Just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

function getAgentName(agentId: string): string {
  const agent = getAllAgents().find((a) => a.id === agentId);
  return agent?.name ?? agentId;
}

function genTriggerId(): string {
  return `trig-custom-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                    */
/* ------------------------------------------------------------------ */

export function AgentTriggers() {
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState<TriggerType | "all">("all");
  const [selectedTrigger, setSelectedTrigger] = useState<TriggerDef | null>(null);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editingTrigger, setEditingTrigger] = useState<TriggerDef | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  const allTriggers = useMemo(() => getAllTriggers(), [refreshKey]);

  const filtered = useMemo(() => {
    let results = allTriggers;
    if (typeFilter !== "all") {
      results = results.filter((t) => t.trigger.type === typeFilter);
    }
    if (search.trim()) {
      const q = search.toLowerCase();
      results = results.filter(
        (t) =>
          t.name.toLowerCase().includes(q) ||
          t.description.toLowerCase().includes(q) ||
          t.agentId.toLowerCase().includes(q) ||
          getAgentName(t.agentId).toLowerCase().includes(q)
      );
    }
    return results;
  }, [allTriggers, typeFilter, search]);

  const counts = useMemo(() => {
    const byType: Record<string, number> = {};
    for (const tt of TRIGGER_TYPES) {
      byType[tt.id] = allTriggers.filter((t) => t.trigger.type === tt.id).length;
    }
    const enabled = allTriggers.filter((t) => t.enabled).length;
    return { total: allTriggers.length, enabled, disabled: allTriggers.length - enabled, byType };
  }, [allTriggers]);

  const handleCreate = () => {
    setEditingTrigger(null);
    setEditorOpen(true);
  };

  const handleEdit = (trigger: TriggerDef) => {
    setEditingTrigger(trigger);
    setEditorOpen(true);
  };

  const handleSave = (trigger: TriggerDef) => {
    if (editingTrigger) {
      updateTrigger(trigger.id, trigger);
    } else {
      addTrigger(trigger);
    }
    setEditorOpen(false);
    setSelectedTrigger(null);
    setRefreshKey((k) => k + 1);
  };

  const handleDelete = (id: string) => {
    deleteTrigger(id);
    setEditorOpen(false);
    setSelectedTrigger(null);
    setRefreshKey((k) => k + 1);
  };

  const handleToggle = (id: string) => {
    toggleTrigger(id);
    setRefreshKey((k) => k + 1);
    // Update selected trigger if it matches
    if (selectedTrigger?.id === id) {
      const updated = getAllTriggers().find((t) => t.id === id);
      if (updated) setSelectedTrigger(updated);
    }
  };

  return (
    <div style={{ flex: 1, height: "100%", overflow: "auto", background: "var(--celerex-bg-primary)" }}>
      <div style={{ maxWidth: 1100, margin: "0 auto", padding: "32px 24px" }}>
        {/* Header */}
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
            Agent Triggers
          </h1>
          <button
            onClick={handleCreate}
            style={{
              padding: "8px 20px",
              borderRadius: 8,
              border: "none",
              background: "var(--celerex-accent)",
              color: "#fff",
              fontSize: 13,
              fontWeight: 600,
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
            }}
          >
            + Create Trigger
          </button>
        </div>
        <p style={{ margin: "4px 0 20px", fontSize: 14, color: "var(--celerex-text-secondary)" }}>
          {counts.total} triggers configured — {counts.enabled} enabled, {counts.disabled} disabled
        </p>

        {/* Type Filter Pills */}
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 16 }}>
          <button
            onClick={() => setTypeFilter("all")}
            style={{
              padding: "5px 14px",
              borderRadius: 20,
              border: typeFilter === "all" ? "none" : "1px solid var(--celerex-border)",
              background: typeFilter === "all" ? "var(--celerex-accent)" : "transparent",
              color: typeFilter === "all" ? "#fff" : "var(--celerex-text-secondary)",
              fontSize: 12,
              fontWeight: 500,
              cursor: "pointer",
            }}
          >
            All ({counts.total})
          </button>
          {TRIGGER_TYPES.map((tt) => {
            const isActive = typeFilter === tt.id;
            return (
              <button
                key={tt.id}
                onClick={() => setTypeFilter(isActive ? "all" : tt.id)}
                style={{
                  padding: "5px 14px",
                  borderRadius: 20,
                  border: isActive ? "none" : "1px solid var(--celerex-border)",
                  background: isActive ? "var(--celerex-accent)" : "transparent",
                  color: isActive ? "#fff" : "var(--celerex-text-secondary)",
                  fontSize: 12,
                  fontWeight: 500,
                  cursor: "pointer",
                }}
              >
                {tt.icon} {tt.label} ({counts.byType[tt.id] ?? 0})
              </button>
            );
          })}
        </div>

        {/* Search */}
        <div style={{ marginBottom: 16 }}>
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search triggers..."
            style={{
              width: "100%",
              boxSizing: "border-box",
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              fontSize: 13,
              outline: "none",
            }}
          />
        </div>

        {/* Selected Trigger Detail */}
        {selectedTrigger && (
          <div
            style={{
              marginBottom: 16,
              padding: 20,
              borderRadius: 10,
              border: "1px solid var(--celerex-accent)",
              background: "var(--celerex-bg-sidebar)",
            }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                <span style={{ fontSize: 20 }}>{getTriggerTypeMeta(selectedTrigger.trigger.type).icon}</span>
                <h3 style={{ margin: 0, fontSize: 16, fontWeight: 600, color: "var(--celerex-text-primary)" }}>
                  {selectedTrigger.name}
                </h3>
                <span
                  style={{
                    padding: "2px 10px",
                    borderRadius: 10,
                    fontSize: 11,
                    fontWeight: 600,
                    background: selectedTrigger.enabled ? "#22c55e22" : "#ef444422",
                    color: selectedTrigger.enabled ? "#22c55e" : "#ef4444",
                  }}
                >
                  {selectedTrigger.enabled ? "Enabled" : "Disabled"}
                </span>
              </div>
              <div style={{ display: "flex", gap: 6 }}>
                <button
                  onClick={() => handleEdit(selectedTrigger)}
                  style={{
                    padding: "5px 14px",
                    borderRadius: 6,
                    border: "1px solid var(--celerex-border)",
                    background: "transparent",
                    color: "var(--celerex-text-primary)",
                    fontSize: 12,
                    cursor: "pointer",
                  }}
                >
                  Edit
                </button>
                <button
                  onClick={() => setSelectedTrigger(null)}
                  style={{
                    background: "none",
                    border: "none",
                    fontSize: 18,
                    color: "var(--celerex-text-secondary)",
                    cursor: "pointer",
                  }}
                >
                  x
                </button>
              </div>
            </div>
            <p style={{ margin: "0 0 12px", fontSize: 13, lineHeight: 1.5, color: "var(--celerex-text-secondary)" }}>
              {selectedTrigger.description}
            </p>
            <div style={{ display: "flex", gap: 24, flexWrap: "wrap", fontSize: 12, color: "var(--celerex-text-secondary)" }}>
              <div>
                <span style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>Type: </span>
                {getTriggerTypeMeta(selectedTrigger.trigger.type).label}
              </div>
              <div>
                <span style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>Agent: </span>
                {getAgentName(selectedTrigger.agentId)}
              </div>
              <div>
                <span style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>Last Run: </span>
                {formatLastRun(selectedTrigger.lastRun)}
              </div>
              <div>
                <span style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>Run Count: </span>
                {selectedTrigger.runCount}
              </div>
            </div>
            {/* Type-specific config details */}
            <div
              style={{
                marginTop: 12,
                padding: 12,
                borderRadius: 6,
                background: "var(--celerex-bg-primary)",
                border: "1px solid var(--celerex-border)",
                fontSize: 12,
                fontFamily: "monospace",
                whiteSpace: "pre-wrap",
                color: "var(--celerex-text-primary)",
              }}
            >
              {JSON.stringify(selectedTrigger.trigger[selectedTrigger.trigger.type as keyof TriggerConfig] ?? {}, null, 2)}
            </div>
          </div>
        )}

        {/* Triggers Grid */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))",
            gap: 12,
          }}
        >
          {filtered.map((trigger) => (
            <TriggerCard
              key={trigger.id}
              trigger={trigger}
              isSelected={selectedTrigger?.id === trigger.id}
              onSelect={() => setSelectedTrigger(trigger)}
              onToggle={() => handleToggle(trigger.id)}
              onEdit={() => handleEdit(trigger)}
            />
          ))}
        </div>
        {filtered.length === 0 && (
          <div style={{ textAlign: "center", padding: 48, color: "var(--celerex-text-secondary)", fontSize: 14 }}>
            No triggers found
          </div>
        )}
      </div>

      {/* Editor Dialog */}
      {editorOpen && (
        <TriggerEditorDialog
          trigger={editingTrigger}
          onSave={handleSave}
          onDelete={editingTrigger ? () => handleDelete(editingTrigger.id) : undefined}
          onClose={() => setEditorOpen(false)}
        />
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Trigger Card                                                      */
/* ------------------------------------------------------------------ */

function TriggerCard({
  trigger,
  isSelected,
  onSelect,
  onToggle,
  onEdit,
}: {
  trigger: TriggerDef;
  isSelected: boolean;
  onSelect: () => void;
  onToggle: () => void;
  onEdit: () => void;
}) {
  const meta = getTriggerTypeMeta(trigger.trigger.type);
  const [hovered, setHovered] = useState(false);

  return (
    <div
      onClick={onSelect}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        padding: 16,
        borderRadius: 10,
        border: isSelected ? "1px solid var(--celerex-accent)" : "1px solid var(--celerex-border)",
        background: hovered ? "var(--celerex-bg-secondary)" : "var(--celerex-bg-sidebar)",
        cursor: "pointer",
        transition: "border-color 0.15s, background 0.15s",
        opacity: trigger.enabled ? 1 : 0.65,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
        <span style={{ fontSize: 18 }}>{meta.icon}</span>
        <span
          style={{
            flex: 1,
            fontSize: 14,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {trigger.name}
        </span>
        {/* Toggle switch */}
        <button
          onClick={(e) => {
            e.stopPropagation();
            onToggle();
          }}
          title={trigger.enabled ? "Disable" : "Enable"}
          style={{
            width: 36,
            height: 20,
            borderRadius: 10,
            border: "none",
            cursor: "pointer",
            position: "relative",
            background: trigger.enabled ? "#22c55e" : "#d1d5db",
            transition: "background 0.2s",
            flexShrink: 0,
          }}
        >
          <span
            style={{
              position: "absolute",
              top: 2,
              left: trigger.enabled ? 18 : 2,
              width: 16,
              height: 16,
              borderRadius: "50%",
              background: "#fff",
              transition: "left 0.2s",
            }}
          />
        </button>
      </div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            padding: "2px 8px",
            borderRadius: 8,
            fontSize: 10,
            fontWeight: 600,
            background: "var(--celerex-accent)" + "18",
            color: "var(--celerex-accent)",
          }}
        >
          {meta.label}
        </span>
      </div>
      <div
        style={{
          fontSize: 12,
          color: "var(--celerex-text-secondary)",
          marginBottom: 8,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {getAgentName(trigger.agentId)}
      </div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: 11, color: "var(--celerex-text-secondary)" }}>
        <span>Last: {formatLastRun(trigger.lastRun)}</span>
        <span>{trigger.runCount} runs</span>
        {hovered && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onEdit();
            }}
            style={{
              padding: "2px 10px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: "transparent",
              color: "var(--celerex-text-secondary)",
              fontSize: 11,
              cursor: "pointer",
            }}
          >
            Edit
          </button>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Trigger Editor Dialog                                              */
/* ------------------------------------------------------------------ */

function TriggerEditorDialog({
  trigger,
  onSave,
  onDelete,
  onClose,
}: {
  trigger: TriggerDef | null;
  onSave: (trigger: TriggerDef) => void;
  onDelete?: () => void;
  onClose: () => void;
}) {
  const isNew = !trigger;

  const [name, setName] = useState(trigger?.name ?? "");
  const [description, setDescription] = useState(trigger?.description ?? "");
  const [agentId, setAgentId] = useState(trigger?.agentId ?? "");
  const [agentSearch, setAgentSearch] = useState("");
  const [triggerType, setTriggerType] = useState<TriggerType>(trigger?.trigger.type ?? "schedule");
  const [enabled, setEnabled] = useState(trigger?.enabled ?? true);

  // Type-specific configs
  const [cronExpr, setCronExpr] = useState(trigger?.trigger.schedule?.cron ?? "0 9 * * MON-FRI");
  const [timezone, setTimezone] = useState(trigger?.trigger.schedule?.timezone ?? "UTC");

  const [webhookPath, setWebhookPath] = useState(trigger?.trigger.webhook?.path ?? "/hooks/");
  const [webhookMethod, setWebhookMethod] = useState<"GET" | "POST" | "PUT">(trigger?.trigger.webhook?.method ?? "POST");
  const [webhookSecret, setWebhookSecret] = useState(trigger?.trigger.webhook?.secret ?? "");

  const [fwPaths, setFwPaths] = useState(trigger?.trigger.file_watch?.paths.join(", ") ?? "/data/inbox");
  const [fwPatterns, setFwPatterns] = useState(trigger?.trigger.file_watch?.patterns?.join(", ") ?? "*.md, *.pdf");
  const [fwEvents, setFwEvents] = useState<Set<"create" | "modify" | "delete">>(
    new Set(trigger?.trigger.file_watch?.events ?? ["create"])
  );

  const [eventSource, setEventSource] = useState<EventConfig["source"]>(trigger?.trigger.event?.source ?? "agent_complete");
  const [eventSourceId, setEventSourceId] = useState(trigger?.trigger.event?.sourceId ?? "");
  const [eventFilter, setEventFilter] = useState(trigger?.trigger.event?.filter ?? "");

  const [emailMailbox, setEmailMailbox] = useState(trigger?.trigger.email?.mailbox ?? "");
  const [emailFromFilter, setEmailFromFilter] = useState(trigger?.trigger.email?.from_filter ?? "");
  const [emailSubjectFilter, setEmailSubjectFilter] = useState(trigger?.trigger.email?.subject_filter ?? "");

  const [gitRepo, setGitRepo] = useState(trigger?.trigger.git?.repo ?? "");
  const [gitEvents, setGitEvents] = useState<Set<"push" | "pull_request" | "commit" | "tag" | "release">>(
    new Set(trigger?.trigger.git?.events ?? ["push"])
  );
  const [gitBranch, setGitBranch] = useState(trigger?.trigger.git?.branch_filter ?? "");

  const [queueProvider, setQueueProvider] = useState<QueueConfig["provider"]>(trigger?.trigger.queue?.provider ?? "redis");
  const [queueName, setQueueName] = useState(trigger?.trigger.queue?.queue_name ?? "");
  const [queueUrl, setQueueUrl] = useState(trigger?.trigger.queue?.connection_url ?? "");

  const [manualLabel, setManualLabel] = useState(trigger?.trigger.manual?.label ?? "");
  const [manualConfirm, setManualConfirm] = useState(trigger?.trigger.manual?.confirmation ?? false);

  const agents = useMemo(() => getAllAgents(), []);
  const filteredAgents = useMemo(() => {
    if (!agentSearch.trim()) return agents.slice(0, 20);
    const q = agentSearch.toLowerCase();
    return agents.filter((a) => a.name.toLowerCase().includes(q) || a.id.toLowerCase().includes(q)).slice(0, 20);
  }, [agents, agentSearch]);

  const buildTriggerConfig = (): TriggerConfig => {
    const config: TriggerConfig = { type: triggerType };
    switch (triggerType) {
      case "schedule":
        config.schedule = { cron: cronExpr, timezone: timezone || undefined };
        break;
      case "webhook":
        config.webhook = { path: webhookPath, method: webhookMethod, secret: webhookSecret || undefined };
        break;
      case "file_watch":
        config.file_watch = {
          paths: fwPaths.split(",").map((p) => p.trim()).filter(Boolean),
          patterns: fwPatterns ? fwPatterns.split(",").map((p) => p.trim()).filter(Boolean) : undefined,
          events: [...fwEvents],
        };
        break;
      case "event":
        config.event = { source: eventSource, sourceId: eventSourceId || undefined, filter: eventFilter || undefined };
        break;
      case "email":
        config.email = { mailbox: emailMailbox, from_filter: emailFromFilter || undefined, subject_filter: emailSubjectFilter || undefined };
        break;
      case "git":
        config.git = { repo: gitRepo || undefined, events: [...gitEvents], branch_filter: gitBranch || undefined };
        break;
      case "queue":
        config.queue = { provider: queueProvider, queue_name: queueName, connection_url: queueUrl || undefined };
        break;
      case "manual":
        config.manual = { label: manualLabel || undefined, confirmation: manualConfirm };
        break;
    }
    return config;
  };

  const handleSubmit = () => {
    if (!name.trim() || !agentId) return;
    onSave({
      id: trigger?.id ?? genTriggerId(),
      name: name.trim(),
      description: description.trim(),
      agentId,
      trigger: buildTriggerConfig(),
      enabled,
      lastRun: trigger?.lastRun,
      runCount: trigger?.runCount ?? 0,
    });
  };

  const labelStyle: React.CSSProperties = {
    display: "block",
    fontSize: 12,
    fontWeight: 600,
    color: "var(--celerex-text-primary)",
    marginBottom: 4,
  };

  const inputStyle: React.CSSProperties = {
    width: "100%",
    boxSizing: "border-box",
    padding: "7px 10px",
    borderRadius: 6,
    border: "1px solid var(--celerex-border)",
    background: "var(--celerex-bg-primary)",
    color: "var(--celerex-text-primary)",
    fontSize: 13,
    outline: "none",
  };

  const selectStyle: React.CSSProperties = {
    ...inputStyle,
    cursor: "pointer",
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.45)",
      }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 620,
          maxHeight: "85vh",
          overflow: "auto",
          borderRadius: 12,
          background: "var(--celerex-bg-sidebar)",
          border: "1px solid var(--celerex-border)",
          padding: 28,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 20 }}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
            {isNew ? "Create Trigger" : "Edit Trigger"}
          </h2>
          <button
            onClick={onClose}
            style={{ background: "none", border: "none", fontSize: 20, color: "var(--celerex-text-secondary)", cursor: "pointer" }}
          >
            x
          </button>
        </div>

        {/* Name */}
        <div style={{ marginBottom: 14 }}>
          <label style={labelStyle}>Name</label>
          <input
            style={inputStyle}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Daily Standup Summary"
          />
        </div>

        {/* Description */}
        <div style={{ marginBottom: 14 }}>
          <label style={labelStyle}>Description</label>
          <textarea
            style={{ ...inputStyle, minHeight: 56, resize: "vertical", fontFamily: "inherit" }}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What does this trigger do?"
          />
        </div>

        {/* Agent Picker */}
        <div style={{ marginBottom: 14 }}>
          <label style={labelStyle}>Target Agent</label>
          <input
            style={inputStyle}
            value={agentSearch}
            onChange={(e) => setAgentSearch(e.target.value)}
            placeholder="Search agents..."
          />
          {agentId && (
            <div
              style={{
                marginTop: 4,
                padding: "4px 10px",
                borderRadius: 6,
                background: "var(--celerex-accent)" + "18",
                color: "var(--celerex-accent)",
                fontSize: 12,
                fontWeight: 600,
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              {getAgentName(agentId)}
              <span onClick={() => setAgentId("")} style={{ cursor: "pointer", opacity: 0.7 }}>x</span>
            </div>
          )}
          {agentSearch.trim() && (
            <div
              style={{
                marginTop: 4,
                maxHeight: 160,
                overflow: "auto",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-primary)",
              }}
            >
              {filteredAgents.map((a) => (
                <div
                  key={a.id}
                  onClick={() => {
                    setAgentId(a.id);
                    setAgentSearch("");
                  }}
                  style={{
                    padding: "6px 10px",
                    fontSize: 12,
                    cursor: "pointer",
                    background: a.id === agentId ? "var(--celerex-accent)" + "18" : "transparent",
                    color: "var(--celerex-text-primary)",
                    borderBottom: "1px solid var(--celerex-border)",
                  }}
                >
                  <span style={{ fontWeight: 500 }}>{a.name}</span>
                  <span style={{ marginLeft: 8, opacity: 0.5, fontSize: 11 }}>{a.id}</span>
                </div>
              ))}
              {filteredAgents.length === 0 && (
                <div style={{ padding: "8px 10px", fontSize: 12, color: "var(--celerex-text-secondary)" }}>No agents found</div>
              )}
            </div>
          )}
        </div>

        {/* Trigger Type Selector */}
        <div style={{ marginBottom: 14 }}>
          <label style={labelStyle}>Trigger Type</label>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(4, 1fr)", gap: 6 }}>
            {TRIGGER_TYPES.map((tt) => {
              const isActive = triggerType === tt.id;
              return (
                <button
                  key={tt.id}
                  onClick={() => setTriggerType(tt.id)}
                  style={{
                    padding: "10px 6px",
                    borderRadius: 8,
                    border: isActive ? "2px solid var(--celerex-accent)" : "1px solid var(--celerex-border)",
                    background: isActive ? "var(--celerex-accent)" + "12" : "var(--celerex-bg-primary)",
                    color: isActive ? "var(--celerex-accent)" : "var(--celerex-text-primary)",
                    cursor: "pointer",
                    textAlign: "center",
                    fontSize: 11,
                    fontWeight: isActive ? 600 : 400,
                  }}
                >
                  <div style={{ fontSize: 18, marginBottom: 2 }}>{tt.icon}</div>
                  {tt.label}
                </button>
              );
            })}
          </div>
        </div>

        {/* Type-specific config forms */}
        <div
          style={{
            padding: 14,
            borderRadius: 8,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-bg-primary)",
            marginBottom: 14,
          }}
        >
          <div style={{ ...labelStyle, marginBottom: 10 }}>
            {getTriggerTypeMeta(triggerType).icon} {getTriggerTypeMeta(triggerType).label} Configuration
          </div>

          {triggerType === "schedule" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Cron Expression</label>
                <input style={inputStyle} value={cronExpr} onChange={(e) => setCronExpr(e.target.value)} placeholder="0 9 * * MON-FRI" />
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Timezone</label>
                <input style={inputStyle} value={timezone} onChange={(e) => setTimezone(e.target.value)} placeholder="UTC" />
              </div>
            </>
          )}

          {triggerType === "webhook" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Path</label>
                <input style={inputStyle} value={webhookPath} onChange={(e) => setWebhookPath(e.target.value)} placeholder="/hooks/my-agent" />
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Method</label>
                <select style={selectStyle} value={webhookMethod} onChange={(e) => setWebhookMethod(e.target.value as any)}>
                  <option value="GET">GET</option>
                  <option value="POST">POST</option>
                  <option value="PUT">PUT</option>
                </select>
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Secret (optional)</label>
                <input style={inputStyle} value={webhookSecret} onChange={(e) => setWebhookSecret(e.target.value)} placeholder="HMAC secret" />
              </div>
            </>
          )}

          {triggerType === "file_watch" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Paths (comma-separated)</label>
                <input style={inputStyle} value={fwPaths} onChange={(e) => setFwPaths(e.target.value)} placeholder="/data/inbox, /data/uploads" />
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Patterns (comma-separated, optional)</label>
                <input style={inputStyle} value={fwPatterns} onChange={(e) => setFwPatterns(e.target.value)} placeholder="*.md, *.pdf" />
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Events</label>
                <div style={{ display: "flex", gap: 8 }}>
                  {(["create", "modify", "delete"] as const).map((ev) => (
                    <label key={ev} style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, cursor: "pointer", color: "var(--celerex-text-primary)" }}>
                      <input
                        type="checkbox"
                        checked={fwEvents.has(ev)}
                        onChange={() => {
                          const next = new Set(fwEvents);
                          next.has(ev) ? next.delete(ev) : next.add(ev);
                          setFwEvents(next);
                        }}
                      />
                      {ev}
                    </label>
                  ))}
                </div>
              </div>
            </>
          )}

          {triggerType === "event" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Source</label>
                <select style={selectStyle} value={eventSource} onChange={(e) => setEventSource(e.target.value as any)}>
                  <option value="agent_complete">Agent Complete</option>
                  <option value="document_save">Document Save</option>
                  <option value="pipeline_finish">Pipeline Finish</option>
                  <option value="error">Error</option>
                  <option value="custom">Custom</option>
                </select>
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Source ID (optional)</label>
                <input style={inputStyle} value={eventSourceId} onChange={(e) => setEventSourceId(e.target.value)} placeholder="specific agent or pipeline ID" />
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Filter Expression (optional)</label>
                <input style={inputStyle} value={eventFilter} onChange={(e) => setEventFilter(e.target.value)} placeholder="e.g. status == 'success'" />
              </div>
            </>
          )}

          {triggerType === "email" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Mailbox</label>
                <input style={inputStyle} value={emailMailbox} onChange={(e) => setEmailMailbox(e.target.value)} placeholder="support@example.com" />
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>From Filter (optional)</label>
                <input style={inputStyle} value={emailFromFilter} onChange={(e) => setEmailFromFilter(e.target.value)} placeholder="sender pattern" />
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Subject Filter (optional)</label>
                <input style={inputStyle} value={emailSubjectFilter} onChange={(e) => setEmailSubjectFilter(e.target.value)} placeholder="*help*|*question*" />
              </div>
            </>
          )}

          {triggerType === "git" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Repository (optional)</label>
                <input style={inputStyle} value={gitRepo} onChange={(e) => setGitRepo(e.target.value)} placeholder="org/repo" />
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Events</label>
                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  {(["push", "pull_request", "commit", "tag", "release"] as const).map((ev) => (
                    <label key={ev} style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 12, cursor: "pointer", color: "var(--celerex-text-primary)" }}>
                      <input
                        type="checkbox"
                        checked={gitEvents.has(ev)}
                        onChange={() => {
                          const next = new Set(gitEvents);
                          next.has(ev) ? next.delete(ev) : next.add(ev);
                          setGitEvents(next);
                        }}
                      />
                      {ev.replace("_", " ")}
                    </label>
                  ))}
                </div>
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Branch Filter (optional)</label>
                <input style={inputStyle} value={gitBranch} onChange={(e) => setGitBranch(e.target.value)} placeholder="main, feature/*" />
              </div>
            </>
          )}

          {triggerType === "queue" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Provider</label>
                <select style={selectStyle} value={queueProvider} onChange={(e) => setQueueProvider(e.target.value as any)}>
                  <option value="sqs">AWS SQS</option>
                  <option value="redis">Redis</option>
                  <option value="kafka">Kafka</option>
                  <option value="rabbitmq">RabbitMQ</option>
                </select>
              </div>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Queue Name</label>
                <input style={inputStyle} value={queueName} onChange={(e) => setQueueName(e.target.value)} placeholder="my-queue" />
              </div>
              <div>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Connection URL (optional)</label>
                <input style={inputStyle} value={queueUrl} onChange={(e) => setQueueUrl(e.target.value)} placeholder="redis://localhost:6379" />
              </div>
            </>
          )}

          {triggerType === "manual" && (
            <>
              <div style={{ marginBottom: 10 }}>
                <label style={{ ...labelStyle, fontWeight: 500 }}>Button Label (optional)</label>
                <input style={inputStyle} value={manualLabel} onChange={(e) => setManualLabel(e.target.value)} placeholder="Run Now" />
              </div>
              <div>
                <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, cursor: "pointer", color: "var(--celerex-text-primary)" }}>
                  <input
                    type="checkbox"
                    checked={manualConfirm}
                    onChange={(e) => setManualConfirm(e.target.checked)}
                  />
                  Require confirmation before running
                </label>
              </div>
            </>
          )}
        </div>

        {/* Enable/Disable */}
        <div style={{ marginBottom: 20 }}>
          <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, cursor: "pointer", color: "var(--celerex-text-primary)" }}>
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
            <span style={{ fontWeight: 500 }}>Enabled</span>
          </label>
        </div>

        {/* Actions */}
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <div>
            {onDelete && (
              <button
                onClick={onDelete}
                style={{
                  padding: "8px 16px",
                  borderRadius: 6,
                  border: "1px solid #ef4444",
                  background: "transparent",
                  color: "#ef4444",
                  fontSize: 13,
                  cursor: "pointer",
                }}
              >
                Delete
              </button>
            )}
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <button
              onClick={onClose}
              style={{
                padding: "8px 16px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "transparent",
                color: "var(--celerex-text-primary)",
                fontSize: 13,
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
            <button
              onClick={handleSubmit}
              disabled={!name.trim() || !agentId}
              style={{
                padding: "8px 20px",
                borderRadius: 6,
                border: "none",
                background: !name.trim() || !agentId ? "#9ca3af" : "var(--celerex-accent)",
                color: "#fff",
                fontSize: 13,
                fontWeight: 600,
                cursor: !name.trim() || !agentId ? "not-allowed" : "pointer",
              }}
            >
              {isNew ? "Create" : "Save"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
