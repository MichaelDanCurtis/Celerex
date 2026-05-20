import { useState } from "react";
import { useSetAtom } from "jotai";
import { login } from "../api/client";
import { tokenAtom, authEngagementAtom } from "../state/atoms";

/**
 * Dev-mode signin page. The user types an engagement name and the backend
 * returns a JWT scoped to that engagement. The token is persisted in
 * localStorage via `tokenAtom` so the app survives a refresh.
 *
 * Shown when `tokenAtom` is empty (see App.tsx).
 */
export function LoginPage() {
  const [engagementName, setEngagementName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setToken = useSetAtom(tokenAtom);
  const setAuthEngagement = useSetAtom(authEngagementAtom);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    const trimmed = engagementName.trim();
    if (!trimmed) {
      setError("Engagement name is required");
      return;
    }
    setSubmitting(true);
    try {
      const resp = await login(trimmed);
      // Persisting via atomWithStorage will also write to localStorage,
      // so a reload picks up the same session.
      setToken(resp.token);
      setAuthEngagement(resp.engagement);
      // Don't navigate manually — App.tsx will re-render the workspace
      // layout as soon as tokenAtom is non-empty.
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      // 404 -> "engagement not found" is the most common case
      if (msg.includes("404")) {
        setError(`No engagement named "${trimmed}" exists. Create one first or check the spelling.`);
      } else {
        setError(msg);
      }
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        height: "100vh",
        width: "100vw",
        background: "var(--celerex-bg-primary)",
        color: "var(--celerex-text-primary)",
      }}
    >
      <form
        onSubmit={handleSubmit}
        style={{
          width: 360,
          padding: 32,
          border: "1px solid var(--celerex-border)",
          borderRadius: 12,
          background: "var(--celerex-bg-sidebar)",
          boxShadow: "0 4px 24px rgba(0,0,0,0.08)",
          display: "flex",
          flexDirection: "column",
          gap: 16,
        }}
      >
        <div>
          <h1 style={{ fontSize: 22, fontWeight: 700, margin: 0, letterSpacing: "-0.02em" }}>
            Celerex
          </h1>
          <p style={{ fontSize: 13, color: "var(--celerex-text-secondary)", margin: "4px 0 0 0" }}>
            Sign in to an engagement
          </p>
        </div>

        <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <span style={{ fontSize: 12, fontWeight: 500, color: "var(--celerex-text-secondary)" }}>
            Engagement name
          </span>
          <input
            type="text"
            value={engagementName}
            onChange={(e) => setEngagementName(e.target.value)}
            placeholder="e.g. Acme Co"
            autoFocus
            disabled={submitting}
            style={{
              padding: "8px 10px",
              fontSize: 14,
              borderRadius: 6,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-primary)",
              color: "var(--celerex-text-primary)",
              outline: "none",
            }}
          />
        </label>

        {error && (
          <div
            role="alert"
            style={{
              fontSize: 12,
              padding: "8px 10px",
              borderRadius: 6,
              background: "rgba(239, 68, 68, 0.08)",
              color: "#dc2626",
              border: "1px solid rgba(239, 68, 68, 0.2)",
            }}
          >
            {error}
          </div>
        )}

        <button
          type="submit"
          disabled={submitting || engagementName.trim().length === 0}
          style={{
            padding: "10px 14px",
            fontSize: 14,
            fontWeight: 600,
            borderRadius: 6,
            border: "none",
            cursor: submitting ? "wait" : "pointer",
            background: "var(--celerex-accent)",
            color: "#fff",
            opacity: submitting || engagementName.trim().length === 0 ? 0.6 : 1,
          }}
        >
          {submitting ? "Signing in…" : "Sign in"}
        </button>

        <p style={{ fontSize: 11, color: "var(--celerex-text-secondary)", margin: 0, lineHeight: 1.4 }}>
          Dev-mode signin: no password required. The backend issues a JWT scoped to the engagement you name.
        </p>
      </form>
    </div>
  );
}
