import { useCallback, useEffect, useMemo, useState } from "react";
import { useAtomValue } from "jotai";
import { tokenAtom } from "../state/atoms";
import { listApprovals, submitReview, type ApiApproval } from "../api/client";

/** Coerce a draft (string markdown or arbitrary JSON) into editable text. */
function draftToText(draft: unknown): string {
  if (typeof draft === "string") return draft;
  if (draft == null) return "";
  return JSON.stringify(draft, null, 2);
}

function timeAgo(iso: string): string {
  const then = new Date(iso).getTime();
  const secs = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (secs < 60) return "just now";
  const mins = Math.round(secs / 60);
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs} hr ago`;
  return `${Math.round(hrs / 24)} d ago`;
}

/**
 * Approvals inbox — the human-in-the-loop surface. Lists runs paused at a
 * Review node, shows the agent's draft in an editable pane, and lets a person
 * approve (optionally editing) or request changes, which resumes the run.
 */
export function TaskQueue() {
  const token = useAtomValue(tokenAtom);
  const [items, setItems] = useState<ApiApproval[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [edited, setEdited] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loaded, setLoaded] = useState(false);

  const refresh = useCallback(async () => {
    if (!token) return;
    try {
      const next = await listApprovals(token);
      setItems(next);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoaded(true);
    }
  }, [token]);

  // Initial load + poll so newly-paused runs appear without a manual refresh.
  useEffect(() => {
    void refresh();
    const id = setInterval(() => void refresh(), 6000);
    return () => clearInterval(id);
  }, [refresh]);

  const active = useMemo(
    () => items.find((i) => i.run_id === selected) ?? null,
    [items, selected],
  );

  // Keep the editor's selection valid as the list refreshes.
  useEffect(() => {
    if (selected && !items.some((i) => i.run_id === selected)) {
      setSelected(null);
    }
  }, [items, selected]);

  function openItem(item: ApiApproval) {
    setSelected(item.run_id);
    setEdited(draftToText(item.draft));
    setError(null);
  }

  async function decide(decision: "approve" | "reject") {
    if (!active) return;
    setBusy(true);
    setError(null);
    try {
      await submitReview(
        token,
        active.run_id,
        decision,
        decision === "approve" ? edited : undefined,
      );
      setSelected(null);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ flex: 1, height: "100%", overflow: "auto", background: "var(--celerex-bg-primary)" }}>
      <div style={{ maxWidth: 920, margin: "0 auto", padding: "32px 24px" }}>
        <h1 style={{ margin: 0, fontSize: 22, fontWeight: 700, color: "var(--celerex-text-primary)" }}>
          Approvals
        </h1>
        <p style={{ margin: "4px 0 24px", fontSize: 14, color: "var(--celerex-text-secondary)" }}>
          Drafts your agents paused for human sign-off. Edit if needed, then approve to continue —
          or request changes to stop the flow.
        </p>

        {error && (
          <div
            style={{
              marginBottom: 16,
              padding: "10px 14px",
              borderRadius: 8,
              border: "1px solid #7f1d1d",
              background: "rgba(127,29,29,0.18)",
              color: "#fca5a5",
              fontSize: 13,
            }}
          >
            {error}
          </div>
        )}

        {loaded && items.length === 0 ? (
          <div
            style={{
              padding: "48px 24px",
              textAlign: "center",
              color: "var(--celerex-text-secondary)",
              border: "1px dashed var(--celerex-border)",
              borderRadius: 12,
            }}
          >
            <div style={{ fontSize: 30, marginBottom: 8 }}>✅</div>
            <div style={{ fontWeight: 600, color: "var(--celerex-text-primary)" }}>Nothing to review</div>
            <div style={{ fontSize: 13, marginTop: 4 }}>
              When a flow hits a “Review &amp; approve” node, its draft lands here.
            </div>
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {items.map((item) => {
              const open = item.run_id === selected;
              return (
                <div
                  key={item.run_id}
                  style={{
                    border: `1px solid ${open ? "var(--celerex-accent)" : "var(--celerex-border)"}`,
                    borderRadius: 12,
                    background: "var(--celerex-bg-secondary, #13161f)",
                    overflow: "hidden",
                  }}
                >
                  <button
                    onClick={() => (open ? setSelected(null) : openItem(item))}
                    style={{
                      width: "100%",
                      display: "flex",
                      alignItems: "center",
                      gap: 12,
                      padding: "14px 16px",
                      background: "transparent",
                      border: "none",
                      cursor: "pointer",
                      textAlign: "left",
                      color: "var(--celerex-text-primary)",
                    }}
                  >
                    <span style={{ fontSize: 18 }}>📝</span>
                    <span style={{ flex: 1, minWidth: 0 }}>
                      <span style={{ display: "block", fontSize: 14, fontWeight: 600 }}>
                        {item.flow_name ?? "Flow"} — needs your review
                      </span>
                      <span style={{ display: "block", fontSize: 12.5, color: "var(--celerex-text-secondary)" }}>
                        {item.prompt}
                      </span>
                    </span>
                    <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>
                      {timeAgo(item.created_at)}
                    </span>
                    <span style={{ fontSize: 12, color: "var(--celerex-text-secondary)" }}>{open ? "▲" : "▼"}</span>
                  </button>

                  {open && (
                    <div style={{ padding: "0 16px 16px" }}>
                      <textarea
                        value={edited}
                        onChange={(e) => setEdited(e.target.value)}
                        spellCheck
                        style={{
                          width: "100%",
                          minHeight: 220,
                          resize: "vertical",
                          padding: "12px 14px",
                          borderRadius: 8,
                          border: "1px solid var(--celerex-border)",
                          background: "var(--celerex-bg-primary)",
                          color: "var(--celerex-text-primary)",
                          fontSize: 13.5,
                          lineHeight: 1.6,
                          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                        }}
                      />
                      <div style={{ display: "flex", gap: 10, marginTop: 12, justifyContent: "flex-end" }}>
                        <button
                          data-testid="reject"
                          onClick={() => void decide("reject")}
                          disabled={busy}
                          style={{
                            padding: "8px 16px",
                            borderRadius: 8,
                            border: "1px solid var(--celerex-border)",
                            background: "transparent",
                            color: "var(--celerex-text-secondary)",
                            fontSize: 13,
                            fontWeight: 600,
                            cursor: busy ? "not-allowed" : "pointer",
                          }}
                        >
                          Request changes
                        </button>
                        <button
                          data-testid="approve"
                          onClick={() => void decide("approve")}
                          disabled={busy}
                          style={{
                            padding: "8px 18px",
                            borderRadius: 8,
                            border: "none",
                            background: "#16a34a",
                            color: "#fff",
                            fontSize: 13,
                            fontWeight: 700,
                            opacity: busy ? 0.6 : 1,
                            cursor: busy ? "not-allowed" : "pointer",
                          }}
                        >
                          {busy ? "Working…" : "Approve & continue"}
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
