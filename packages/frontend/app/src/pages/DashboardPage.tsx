import { useCallback, useEffect, useRef, useState } from "react";
import { useAtomValue } from "jotai";
import { tokenAtom } from "../state/atoms";
import { fetchMetricsSummary, type ApiMetricsSummary } from "../api/client";

const REFRESH_INTERVAL_MS = 10_000;

/**
 * Workspace Dashboard — landing page after sign-in.
 *
 * Auto-refreshes `GET /api/metrics/summary` every 10s and renders prominent
 * stat tiles, a 24h success-rate hero, and provider/agent-type breakdowns.
 */
export function DashboardPage() {
  const token = useAtomValue(tokenAtom);
  const [summary, setSummary] = useState<ApiMetricsSummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);
  // Hold the in-flight token so a stale response from a previous fetch can't
  // overwrite a fresh one if the user spams Refresh.
  const fetchSeqRef = useRef(0);

  const load = useCallback(async () => {
    if (!token) return;
    const seq = ++fetchSeqRef.current;
    setLoading(true);
    try {
      const data = await fetchMetricsSummary(token);
      if (seq !== fetchSeqRef.current) return;
      setSummary(data);
      setError(null);
      setLastUpdated(new Date());
    } catch (e: unknown) {
      if (seq !== fetchSeqRef.current) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (seq === fetchSeqRef.current) setLoading(false);
    }
  }, [token]);

  // Initial fetch + 10s polling.
  useEffect(() => {
    if (!token) return;
    load();
    const id = setInterval(load, REFRESH_INTERVAL_MS);
    return () => clearInterval(id);
  }, [token, load]);

  const successRate =
    summary && summary.runs_24h > 0
      ? Math.round((summary.runs_succeeded_24h / summary.runs_24h) * 100)
      : null;

  const reasoning =
    summary?.members_by_agent_type?.["reasoning"] ??
    summary?.members_by_agent_type?.["Reasoning"] ??
    0;
  const writer =
    summary?.members_by_agent_type?.["writer"] ??
    summary?.members_by_agent_type?.["Writer"] ??
    summary?.members_by_agent_type?.["document_servicer"] ??
    0;

  return (
    <div
      data-testid="dashboard-page"
      style={{
        flex: 1,
        overflow: "auto",
        padding: "40px 24px",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 1100, margin: "0 auto" }}>
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: 16,
            marginBottom: 24,
          }}
        >
          <div>
            <h1
              style={{
                fontSize: 28,
                fontWeight: 700,
                color: "var(--celerex-text-primary)",
                margin: 0,
                letterSpacing: "-0.02em",
              }}
            >
              Workspace overview
            </h1>
            <p
              style={{
                fontSize: 13,
                color: "var(--celerex-text-secondary)",
                margin: "4px 0 0",
              }}
            >
              Live counts of your engagement, refreshed every 10 seconds.
            </p>
          </div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
            }}
          >
            <span
              data-testid="dashboard-last-updated"
              style={{
                fontSize: 12,
                color: "var(--celerex-text-secondary)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              {lastUpdated
                ? `Last updated: ${formatTime(lastUpdated)}`
                : loading
                  ? "Loading…"
                  : "Not yet loaded"}
            </span>
            <button
              data-testid="dashboard-refresh"
              onClick={load}
              disabled={loading || !token}
              style={{
                padding: "6px 14px",
                borderRadius: 6,
                border: "1px solid var(--celerex-border)",
                background: "transparent",
                color: "var(--celerex-text-primary)",
                cursor: loading || !token ? "not-allowed" : "pointer",
                fontSize: 13,
                fontWeight: 500,
                opacity: loading || !token ? 0.6 : 1,
              }}
            >
              {loading ? "Refreshing…" : "Refresh"}
            </button>
          </div>
        </div>

        {/* Error banner */}
        {error && (
          <div
            data-testid="dashboard-error"
            role="alert"
            style={{
              padding: "10px 14px",
              borderRadius: 8,
              background: "rgba(220, 38, 38, 0.08)",
              color: "#dc2626",
              border: "1px solid rgba(220, 38, 38, 0.3)",
              fontSize: 13,
              marginBottom: 16,
            }}
          >
            Failed to load metrics: {error}
          </div>
        )}

        {/* Empty / signed-out */}
        {!token && (
          <div
            style={{
              fontSize: 13,
              fontStyle: "italic",
              color: "var(--celerex-text-secondary)",
            }}
          >
            Sign in to view workspace metrics.
          </div>
        )}

        {/* Initial-loading skeleton */}
        {token && !summary && !error && (
          <div
            data-testid="dashboard-loading"
            style={{
              fontSize: 13,
              fontStyle: "italic",
              color: "var(--celerex-text-secondary)",
              padding: "40px 0",
            }}
          >
            Loading workspace metrics…
          </div>
        )}

        {summary && (
          <>
            {/* Hero: 24h success rate */}
            <div
              data-testid="dashboard-hero"
              style={{
                padding: "24px 28px",
                marginBottom: 24,
                borderRadius: 12,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-sidebar)",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: 24,
              }}
            >
              <div style={{ minWidth: 0 }}>
                <div
                  style={{
                    fontSize: 12,
                    fontWeight: 600,
                    textTransform: "uppercase",
                    letterSpacing: "0.06em",
                    color: "var(--celerex-text-secondary)",
                    marginBottom: 6,
                  }}
                >
                  24h success rate
                </div>
                <div
                  data-testid="dashboard-success-rate"
                  style={{
                    fontSize: 56,
                    fontWeight: 700,
                    lineHeight: 1,
                    color:
                      successRate === null
                        ? "var(--celerex-text-secondary)"
                        : successRate >= 90
                          ? "#16a34a"
                          : successRate >= 70
                            ? "#d97706"
                            : "#dc2626",
                    letterSpacing: "-0.03em",
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {successRate === null ? "—" : `${successRate}%`}
                </div>
                <div
                  style={{
                    fontSize: 12,
                    color: "var(--celerex-text-secondary)",
                    marginTop: 8,
                  }}
                >
                  {summary.runs_succeeded_24h} succeeded ·{" "}
                  {summary.runs_failed_24h} failed · {summary.runs_24h} total
                </div>
              </div>
              <div
                style={{
                  fontSize: 12,
                  color: "var(--celerex-text-secondary)",
                  textAlign: "right",
                }}
              >
                <div style={{ marginBottom: 6 }}>Agent members</div>
                <div
                  data-testid="dashboard-agent-types"
                  style={{
                    fontSize: 14,
                    color: "var(--celerex-text-primary)",
                  }}
                >
                  Reasoning: <strong>{reasoning}</strong>
                  <span style={{ opacity: 0.4, margin: "0 8px" }}>·</span>
                  Writer: <strong>{writer}</strong>
                </div>
              </div>
            </div>

            {/* Stat tiles grid */}
            <div
              data-testid="dashboard-tiles"
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
                gap: 16,
                marginBottom: 24,
              }}
            >
              <StatTile
                label="Members"
                value={summary.members_total}
                testId="tile-members"
              />
              <StatTile
                label="Projects"
                value={summary.projects_total}
                testId="tile-projects"
              />
              <StatTile
                label="Schedules"
                value={`${summary.schedules_enabled} / ${summary.schedules_total}`}
                hint="enabled / total"
                testId="tile-schedules"
              />
              <StatTile
                label="Runs (24h)"
                value={summary.runs_24h}
                testId="tile-runs"
              />
              <StatTile
                label="Documents"
                value={summary.docs_total}
                testId="tile-docs"
              />
              <StatTile
                label="Plugins"
                value={summary.plugins_installed}
                testId="tile-plugins"
              />
            </div>

            {/* Provider badges */}
            <div
              style={{
                padding: "18px 20px",
                borderRadius: 12,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-sidebar)",
              }}
            >
              <div
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  textTransform: "uppercase",
                  letterSpacing: "0.06em",
                  color: "var(--celerex-text-secondary)",
                  marginBottom: 10,
                }}
              >
                Providers configured
              </div>
              {summary.providers_configured.length === 0 ? (
                <span
                  style={{
                    fontSize: 13,
                    fontStyle: "italic",
                    color: "var(--celerex-text-secondary)",
                  }}
                >
                  No providers configured.
                </span>
              ) : (
                <div
                  data-testid="dashboard-providers"
                  style={{ display: "flex", flexWrap: "wrap", gap: 6 }}
                >
                  {summary.providers_configured.map((id) => (
                    <span
                      key={id}
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        fontSize: 12,
                        fontFamily: "monospace",
                        padding: "4px 10px",
                        borderRadius: 12,
                        background: "rgba(59, 130, 246, 0.10)",
                        color: "var(--celerex-accent)",
                        border: "1px solid rgba(59, 130, 246, 0.3)",
                      }}
                    >
                      {id}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function StatTile({
  label,
  value,
  hint,
  testId,
}: {
  label: string;
  value: number | string;
  hint?: string;
  testId?: string;
}) {
  return (
    <div
      data-testid={testId}
      style={{
        padding: "18px 20px",
        borderRadius: 12,
        border: "1px solid var(--celerex-border)",
        background: "var(--celerex-bg-sidebar)",
        display: "flex",
        flexDirection: "column",
        gap: 6,
        minHeight: 92,
      }}
    >
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          color: "var(--celerex-text-secondary)",
        }}
      >
        {label}
      </div>
      <div
        style={{
          fontSize: 32,
          fontWeight: 700,
          color: "var(--celerex-text-primary)",
          lineHeight: 1.1,
          letterSpacing: "-0.02em",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {value}
      </div>
      {hint && (
        <div
          style={{
            fontSize: 11,
            color: "var(--celerex-text-secondary)",
          }}
        >
          {hint}
        </div>
      )}
    </div>
  );
}

function formatTime(d: Date): string {
  return d.toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}
