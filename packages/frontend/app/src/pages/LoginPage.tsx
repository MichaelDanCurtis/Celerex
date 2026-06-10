import { useState } from "react";
import { useSetAtom } from "jotai";
import { login, signup } from "../api/client";
import { tokenAtom, authEngagementAtom, activeViewAtom } from "../state/atoms";

/**
 * Real auth: email + password. New users sign up (which also provisions their
 * workspace/engagement); returning users sign in. The session JWT is persisted
 * via `tokenAtom`, so a refresh keeps the session. Shown when `tokenAtom` is
 * empty (see App.tsx).
 */
export function LoginPage() {
  const [mode, setMode] = useState<"signin" | "signup">("signin");
  const [email, setEmail] = useState("");
  const [pw, setPw] = useState("");
  const [engagementName, setEngagementName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const setToken = useSetAtom(tokenAtom);
  const setAuthEngagement = useSetAtom(authEngagementAtom);
  const setActiveView = useSetAtom(activeViewAtom);

  const isSignup = mode === "signup";
  const canSubmit =
    email.includes("@") && pw.length >= 8 && (!isSignup || engagementName.trim().length > 0);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    if (!canSubmit) {
      setError(
        isSignup
          ? "Enter an email, an 8+ character password, and a workspace name."
          : "Enter your email and password (8+ characters).",
      );
      return;
    }
    setSubmitting(true);
    try {
      const resp = isSignup
        ? await signup(email.trim(), pw, engagementName.trim())
        : await login(email.trim(), pw);
      setToken(resp.token);
      setAuthEngagement(resp.engagement);
      setActiveView("dashboard");
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.includes("401")) setError("Invalid email or password.");
      else if (msg.includes("409")) setError("An account with that email already exists — sign in instead.");
      else if (msg.includes("400")) setError("Check your email and that the password is at least 8 characters.");
      else setError(msg);
    } finally {
      setSubmitting(false);
    }
  }

  const inputStyle: React.CSSProperties = {
    padding: "8px 10px",
    fontSize: 14,
    borderRadius: 6,
    border: "1px solid var(--celerex-border)",
    background: "var(--celerex-bg-primary)",
    color: "var(--celerex-text-primary)",
    outline: "none",
  };
  const labelText: React.CSSProperties = { fontSize: 12, fontWeight: 500, color: "var(--celerex-text-secondary)" };

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
          gap: 14,
        }}
      >
        <div>
          <h1 style={{ fontSize: 22, fontWeight: 700, margin: 0, letterSpacing: "-0.02em" }}>Celerex</h1>
          <p style={{ fontSize: 13, color: "var(--celerex-text-secondary)", margin: "4px 0 0 0" }}>
            {isSignup ? "Create your account" : "Sign in to your workspace"}
          </p>
        </div>

        <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <span style={labelText}>Email</span>
          <input
            type="email"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="you@company.com"
            autoFocus
            autoComplete="email"
            disabled={submitting}
            style={inputStyle}
          />
        </label>

        <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          <span style={labelText}>Password</span>
          <input
            type="password"
            value={pw}
            onChange={(e) => setPw(e.target.value)}
            placeholder="At least 8 characters"
            autoComplete={isSignup ? "new-password" : "current-password"}
            disabled={submitting}
            style={inputStyle}
          />
        </label>

        {isSignup && (
          <label style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            <span style={labelText}>Workspace name</span>
            <input
              type="text"
              value={engagementName}
              onChange={(e) => setEngagementName(e.target.value)}
              placeholder="e.g. Acme Co"
              disabled={submitting}
              style={inputStyle}
            />
          </label>
        )}

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
          disabled={submitting || !canSubmit}
          style={{
            padding: "10px 14px",
            fontSize: 14,
            fontWeight: 600,
            borderRadius: 6,
            border: "none",
            cursor: submitting ? "wait" : "pointer",
            background: "var(--celerex-accent)",
            color: "#fff",
            opacity: submitting || !canSubmit ? 0.6 : 1,
          }}
        >
          {submitting ? (isSignup ? "Creating…" : "Signing in…") : isSignup ? "Create account" : "Sign in"}
        </button>

        <button
          type="button"
          onClick={() => {
            setMode(isSignup ? "signin" : "signup");
            setError(null);
          }}
          style={{
            background: "none",
            border: "none",
            color: "var(--celerex-accent)",
            fontSize: 12.5,
            cursor: "pointer",
            padding: 0,
          }}
        >
          {isSignup ? "Already have an account? Sign in" : "New to Celerex? Create an account"}
        </button>
      </form>
    </div>
  );
}
