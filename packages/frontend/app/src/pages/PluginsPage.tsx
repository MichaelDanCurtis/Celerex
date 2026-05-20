import { useEffect, useMemo, useState } from "react";
import { useAtomValue } from "jotai";
import { tokenAtom } from "../state/atoms";
import {
  listInstalledPlugins,
  installPlugin,
  setPluginCredentials,
  type ApiPluginInstall,
  type PluginManifest,
} from "../api/client";

type CredRow = { key: string; value: string };
type Toast = { kind: "success" | "error"; message: string } | null;

export function PluginsPage() {
  const token = useAtomValue(tokenAtom);

  const [plugins, setPlugins] = useState<ApiPluginInstall[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installOpen, setInstallOpen] = useState(false);

  const selected = useMemo(
    () => plugins.find((p) => p.id === selectedId) ?? null,
    [plugins, selectedId],
  );

  async function refresh() {
    if (!token) {
      setPlugins([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const rows = await listInstalledPlugins(token);
      setPlugins(rows);
      if (rows.length > 0 && (!selectedId || !rows.some((r) => r.id === selectedId))) {
        setSelectedId(rows[0].id);
      } else if (rows.length === 0) {
        setSelectedId(null);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load plugins");
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token]);

  // Gate: missing token
  if (!token) {
    return (
      <div style={{ padding: 16, flex: 1 }}>
        <Banner kind="warn">
          You are not signed in. Set a JWT in Settings to manage plugins.
        </Banner>
      </div>
    );
  }

  return (
    <div
      data-testid="plugins-page"
      style={{ flex: 1, display: "flex", overflow: "hidden" }}
    >
      {/* Left: list */}
      <div
        style={{
          width: 320,
          borderRight: "1px solid var(--celerex-border)",
          overflow: "auto",
          padding: 16,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "baseline",
            justifyContent: "space-between",
            marginBottom: 8,
          }}
        >
          <h2 style={{ margin: 0, fontSize: 16 }}>Plugins</h2>
          {loading && (
            <span
              style={{ fontSize: 11, color: "var(--celerex-text-secondary)" }}
            >
              Loading&hellip;
            </span>
          )}
        </div>

        <button
          data-testid="plugins-install-btn"
          onClick={() => setInstallOpen(true)}
          style={{
            padding: "8px 12px",
            borderRadius: 6,
            border: "1px solid var(--celerex-accent)",
            background: "var(--celerex-accent)",
            color: "#fff",
            fontSize: 13,
            fontWeight: 500,
            cursor: "pointer",
            marginBottom: 12,
          }}
        >
          + Install plugin
        </button>

        {error && <Banner kind="error">Failed to load: {error}</Banner>}

        {!loading && !error && plugins.length === 0 && (
          <div
            data-testid="plugins-empty"
            style={{
              padding: 16,
              color: "var(--celerex-text-secondary)",
              fontStyle: "italic",
              fontSize: 13,
              textAlign: "center",
            }}
          >
            No plugins installed.
          </div>
        )}

        {plugins.map((p) => {
          const inboundCount = p.manifest?.inbound?.length ?? 0;
          const outboundCount = p.manifest?.outbound?.length ?? 0;
          const active = selectedId === p.id;
          return (
            <div
              key={p.id}
              data-testid={`plugins-row-${p.id}`}
              onClick={() => setSelectedId(p.id)}
              style={{
                padding: "8px 10px",
                borderRadius: 6,
                cursor: "pointer",
                marginBottom: 4,
                background: active
                  ? "rgba(59, 130, 246, 0.10)"
                  : "transparent",
                color: active
                  ? "var(--celerex-accent)"
                  : "var(--celerex-text-primary)",
                border: active
                  ? "1px solid rgba(59, 130, 246, 0.3)"
                  : "1px solid transparent",
              }}
            >
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {p.manifest?.name || p.plugin_id || "Unnamed"}
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--celerex-text-secondary)",
                  marginTop: 2,
                  display: "flex",
                  justifyContent: "space-between",
                  gap: 8,
                }}
              >
                <span style={{ fontFamily: "monospace" }}>{p.plugin_id}</span>
                <span>
                  in {inboundCount} / out {outboundCount}
                </span>
              </div>
            </div>
          );
        })}
      </div>

      {/* Right: detail */}
      <div style={{ flex: 1, overflow: "auto", display: "flex" }}>
        {selected ? (
          <PluginDetail
            key={selected.id}
            token={token}
            plugin={selected}
            onCredentialsSaved={() => refresh()}
          />
        ) : (
          <div
            style={{
              flex: 1,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              color: "var(--celerex-text-secondary)",
              fontStyle: "italic",
            }}
          >
            Select a plugin to view its details.
          </div>
        )}
      </div>

      {installOpen && (
        <InstallModal
          token={token}
          onClose={() => setInstallOpen(false)}
          onInstalled={(newId) => {
            setInstallOpen(false);
            void refresh().then(() => setSelectedId(newId));
          }}
        />
      )}
    </div>
  );
}

/* ---------------------------------------------------------------- detail */

function PluginDetail({
  token,
  plugin,
  onCredentialsSaved,
}: {
  token: string;
  plugin: ApiPluginInstall;
  onCredentialsSaved: () => void;
}) {
  const [direction, setDirection] = useState<"inbound" | "outbound">("outbound");
  const [rows, setRows] = useState<CredRow[]>([{ key: "", value: "" }]);
  const [saving, setSaving] = useState(false);
  const [toast, setToast] = useState<Toast>(null);

  const inbound = plugin.manifest?.inbound ?? [];
  const outbound = plugin.manifest?.outbound ?? [];

  function updateRow(i: number, field: "key" | "value", v: string) {
    setRows((prev) => prev.map((r, idx) => (idx === i ? { ...r, [field]: v } : r)));
  }
  function addRow() {
    setRows((prev) => [...prev, { key: "", value: "" }]);
  }
  function removeRow(i: number) {
    setRows((prev) => (prev.length <= 1 ? prev : prev.filter((_, idx) => idx !== i)));
  }

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSaving(true);
    setToast(null);
    try {
      const payload: Record<string, string> = {};
      for (const r of rows) {
        const k = r.key.trim();
        if (!k) continue;
        payload[k] = r.value;
      }
      if (Object.keys(payload).length === 0) {
        setToast({ kind: "error", message: "Provide at least one credential field." });
        return;
      }
      await setPluginCredentials(token, plugin.id, direction, payload);
      setToast({ kind: "success", message: "Credentials saved." });
      onCredentialsSaved();
    } catch (err) {
      setToast({
        kind: "error",
        message: err instanceof Error ? err.message : "Failed to save credentials",
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div style={{ flex: 1, padding: 20, overflow: "auto" }}>
      <div style={{ marginBottom: 16 }}>
        <h2 style={{ margin: 0, fontSize: 18 }}>
          {plugin.manifest?.name || plugin.plugin_id}
        </h2>
        <div
          style={{
            fontSize: 12,
            color: "var(--celerex-text-secondary)",
            marginTop: 2,
            display: "flex",
            gap: 12,
            flexWrap: "wrap",
          }}
        >
          <span>
            id: <code>{plugin.plugin_id}</code>
          </span>
          {plugin.manifest?.version && (
            <span>
              v<code>{plugin.manifest.version}</code>
            </span>
          )}
          <span>install: <code>{plugin.id}</code></span>
        </div>
      </div>

      {/* Capabilities */}
      <section style={{ marginBottom: 24 }}>
        <h3 style={{ fontSize: 13, margin: "0 0 8px" }}>Capabilities</h3>
        <CapList title="Inbound" items={inbound} emptyLabel="No inbound capabilities" />
        <CapList title="Outbound" items={outbound} emptyLabel="No outbound capabilities" />
      </section>

      {/* Credentials */}
      <section>
        <h3 style={{ fontSize: 13, margin: "0 0 8px" }}>Credentials</h3>
        <form onSubmit={handleSubmit} data-testid="plugins-cred-form">
          <div style={{ marginBottom: 10 }}>
            <label
              style={{
                fontSize: 12,
                color: "var(--celerex-text-secondary)",
                display: "block",
                marginBottom: 4,
              }}
            >
              Direction
            </label>
            <select
              value={direction}
              data-testid="plugins-cred-direction"
              onChange={(e) =>
                setDirection(e.target.value === "inbound" ? "inbound" : "outbound")
              }
              style={{
                padding: "6px 8px",
                fontSize: 12,
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-primary)",
                color: "var(--celerex-text-primary)",
              }}
            >
              <option value="outbound">outbound</option>
              <option value="inbound">inbound</option>
            </select>
          </div>

          {rows.map((row, i) => (
            <div
              key={i}
              style={{
                display: "flex",
                gap: 8,
                marginBottom: 6,
                alignItems: "center",
              }}
            >
              <input
                data-testid={`plugins-cred-key-${i}`}
                placeholder="key"
                value={row.key}
                onChange={(e) => updateRow(i, "key", e.target.value)}
                style={{
                  flex: 1,
                  padding: "6px 8px",
                  fontSize: 12,
                  border: "1px solid var(--celerex-border)",
                  borderRadius: 6,
                  background: "var(--celerex-bg-primary)",
                  color: "var(--celerex-text-primary)",
                }}
              />
              <input
                data-testid={`plugins-cred-value-${i}`}
                placeholder="value"
                value={row.value}
                onChange={(e) => updateRow(i, "value", e.target.value)}
                style={{
                  flex: 2,
                  padding: "6px 8px",
                  fontSize: 12,
                  border: "1px solid var(--celerex-border)",
                  borderRadius: 6,
                  background: "var(--celerex-bg-primary)",
                  color: "var(--celerex-text-primary)",
                }}
              />
              <button
                type="button"
                onClick={() => removeRow(i)}
                disabled={rows.length <= 1}
                title="Remove row"
                style={{
                  padding: "4px 10px",
                  borderRadius: 6,
                  border: "1px solid var(--celerex-border)",
                  background: "transparent",
                  color: "var(--celerex-text-secondary)",
                  cursor: rows.length <= 1 ? "not-allowed" : "pointer",
                  fontSize: 14,
                  lineHeight: 1,
                  opacity: rows.length <= 1 ? 0.4 : 1,
                }}
              >
                &minus;
              </button>
            </div>
          ))}

          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <button
              type="button"
              onClick={addRow}
              style={{
                padding: "6px 12px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "transparent",
                color: "var(--celerex-text-secondary)",
                cursor: "pointer",
                fontSize: 12,
              }}
            >
              + Add field
            </button>
            <button
              type="submit"
              disabled={saving}
              data-testid="plugins-cred-save"
              style={{
                padding: "6px 14px",
                borderRadius: 6,
                border: "1px solid var(--celerex-accent)",
                background: "var(--celerex-accent)",
                color: "#fff",
                fontSize: 12,
                fontWeight: 500,
                cursor: saving ? "wait" : "pointer",
                opacity: saving ? 0.7 : 1,
              }}
            >
              {saving ? "Saving…" : "Save credentials"}
            </button>
          </div>

          {toast && (
            <div style={{ marginTop: 12 }}>
              <Banner kind={toast.kind === "success" ? "success" : "error"}>
                {toast.message}
              </Banner>
            </div>
          )}
        </form>
      </section>
    </div>
  );
}

function CapList({
  title,
  items,
  emptyLabel,
}: {
  title: string;
  items: { id: string; kind: string }[];
  emptyLabel: string;
}) {
  return (
    <div style={{ marginBottom: 8 }}>
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          color: "var(--celerex-text-secondary)",
          marginBottom: 4,
        }}
      >
        {title}
      </div>
      {items.length === 0 ? (
        <div
          style={{
            fontSize: 12,
            color: "var(--celerex-text-secondary)",
            fontStyle: "italic",
          }}
        >
          {emptyLabel}
        </div>
      ) : (
        <ul style={{ margin: 0, padding: 0, listStyle: "none" }}>
          {items.map((cap) => (
            <li
              key={cap.id}
              style={{
                fontSize: 12,
                fontFamily: "monospace",
                padding: "3px 0",
                color: "var(--celerex-text-primary)",
              }}
            >
              {cap.id}{" "}
              <span style={{ color: "var(--celerex-text-secondary)" }}>
                &lt;{cap.kind}&gt;
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ---------------------------------------------------------------- modal */

function InstallModal({
  token,
  onClose,
  onInstalled,
}: {
  token: string;
  onClose: () => void;
  onInstalled: (newInstallId: string) => void;
}) {
  const [id, setId] = useState("");
  const [name, setName] = useState("");
  const [version, setVersion] = useState("0.1.0");
  const [manifestText, setManifestText] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);

    let manifest: PluginManifest;
    if (manifestText.trim()) {
      try {
        const parsed = JSON.parse(manifestText) as Partial<PluginManifest>;
        manifest = {
          id: parsed.id ?? id.trim(),
          name: parsed.name ?? name.trim(),
          version: parsed.version ?? (version.trim() || "0.1.0"),
          inbound: parsed.inbound ?? [],
          outbound: parsed.outbound ?? [],
        };
      } catch (e) {
        setError(
          `Invalid JSON: ${e instanceof Error ? e.message : String(e)}`,
        );
        return;
      }
    } else {
      if (!id.trim() || !name.trim()) {
        setError("id and name are required.");
        return;
      }
      manifest = {
        id: id.trim(),
        name: name.trim(),
        version: version.trim() || "0.1.0",
        inbound: [],
        outbound: [],
      };
    }

    if (!manifest.id || !manifest.name) {
      setError("manifest.id and manifest.name are required.");
      return;
    }

    setSubmitting(true);
    try {
      const result = await installPlugin(token, manifest);
      onInstalled(result.id);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to install");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.4)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 1000,
      }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        data-testid="plugins-install-modal"
        style={{
          width: 480,
          maxWidth: "90vw",
          maxHeight: "85vh",
          overflow: "auto",
          background: "var(--celerex-bg-primary)",
          color: "var(--celerex-text-primary)",
          border: "1px solid var(--celerex-border)",
          borderRadius: 8,
          padding: 20,
          boxShadow: "0 10px 40px rgba(0,0,0,0.2)",
        }}
      >
        <h2 style={{ margin: "0 0 12px", fontSize: 16 }}>Install plugin</h2>
        <form onSubmit={handleSubmit}>
          <Field label="Plugin id">
            <input
              data-testid="plugins-install-id"
              value={id}
              onChange={(e) => setId(e.target.value)}
              placeholder="e.g. teams"
              style={inputStyle}
            />
          </Field>
          <Field label="Display name">
            <input
              data-testid="plugins-install-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Microsoft Teams"
              style={inputStyle}
            />
          </Field>
          <Field label="Version">
            <input
              data-testid="plugins-install-version"
              value={version}
              onChange={(e) => setVersion(e.target.value)}
              placeholder="0.1.0"
              style={inputStyle}
            />
          </Field>
          <Field label="Manifest JSON (optional — overrides fields above)">
            <textarea
              data-testid="plugins-install-manifest"
              value={manifestText}
              onChange={(e) => setManifestText(e.target.value)}
              placeholder='{"id":"teams","name":"Teams","version":"0.1.0","inbound":[],"outbound":[]}'
              rows={8}
              style={{
                ...inputStyle,
                fontFamily: "monospace",
                fontSize: 12,
                resize: "vertical",
              }}
            />
          </Field>

          {error && <Banner kind="error">{error}</Banner>}

          <div
            style={{
              display: "flex",
              justifyContent: "flex-end",
              gap: 8,
              marginTop: 12,
            }}
          >
            <button
              type="button"
              onClick={onClose}
              style={{
                padding: "8px 14px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "transparent",
                color: "var(--celerex-text-secondary)",
                cursor: "pointer",
                fontSize: 13,
              }}
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              data-testid="plugins-install-submit"
              style={{
                padding: "8px 14px",
                borderRadius: 6,
                border: "1px solid var(--celerex-accent)",
                background: "var(--celerex-accent)",
                color: "#fff",
                cursor: submitting ? "wait" : "pointer",
                fontSize: 13,
                fontWeight: 500,
                opacity: submitting ? 0.7 : 1,
              }}
            >
              {submitting ? "Installing…" : "Install"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 13,
  border: "1px solid var(--celerex-border)",
  borderRadius: 6,
  background: "var(--celerex-bg-primary)",
  color: "var(--celerex-text-primary)",
  boxSizing: "border-box",
};

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label style={{ display: "block", marginBottom: 10 }}>
      <span
        style={{
          fontSize: 12,
          color: "var(--celerex-text-secondary)",
          display: "block",
          marginBottom: 4,
        }}
      >
        {label}
      </span>
      {children}
    </label>
  );
}

/* ---------------------------------------------------------------- shared */

function Banner({
  kind,
  children,
}: {
  kind: "warn" | "error" | "success";
  children: React.ReactNode;
}) {
  const palette =
    kind === "error"
      ? { bg: "rgba(220, 38, 38, 0.08)", fg: "#dc2626", border: "#dc2626" }
      : kind === "success"
        ? { bg: "rgba(34, 197, 94, 0.08)", fg: "#16a34a", border: "#22c55e" }
        : { bg: "rgba(217, 119, 6, 0.08)", fg: "#b45309", border: "#d97706" };

  return (
    <div
      style={{
        padding: "10px 12px",
        background: palette.bg,
        color: palette.fg,
        border: `1px solid ${palette.border}`,
        borderRadius: 6,
        fontSize: 13,
        marginBottom: 12,
      }}
    >
      {children}
    </div>
  );
}
