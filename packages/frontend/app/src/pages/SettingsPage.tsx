import { useEffect, useMemo, useState } from "react";
import { useAtom, useAtomValue, useSetAtom } from "jotai";
import {
  tokenAtom,
  engagementsAtom,
  engagementsForActiveProjectAtom,
  activeEngagementIdAtom,
  activeProjectIdAtom,
  projectsAtom,
  activeViewAtom,
} from "../state/atoms";
import {
  API_BASE,
  listProviders,
  listInstalledPlugins,
  type ApiPluginInstall,
} from "../api/client";
import { EngagementManager } from "../layout/EngagementManager";
import { ProviderKeysSettings } from "../components/ProviderKeysSettings";
import { ApproveDeviceSettings } from "../components/ApproveDeviceSettings";

const APP_VERSION = "v0.1-overnight";

export function SettingsPage() {
  const [token, setToken] = useAtom(tokenAtom);
  const allEngagements = useAtomValue(engagementsAtom);
  const visibleEngagements = useAtomValue(engagementsForActiveProjectAtom);
  const activeEngagementId = useAtomValue(activeEngagementIdAtom);
  const activeProjectId = useAtomValue(activeProjectIdAtom);
  const projects = useAtomValue(projectsAtom);
  const setActiveView = useSetAtom(activeViewAtom);

  const [providers, setProviders] = useState<string[] | null>(null);
  const [providersError, setProvidersError] = useState<string | null>(null);
  const [providersLoading, setProvidersLoading] = useState(false);

  const [plugins, setPlugins] = useState<ApiPluginInstall[] | null>(null);
  const [pluginsError, setPluginsError] = useState<string | null>(null);
  const [pluginsLoading, setPluginsLoading] = useState(false);

  const [engagementManagerOpen, setEngagementManagerOpen] = useState(false);

  // Resolve the engagement to display — same precedence as DocsPage.
  const effectiveEngagement = useMemo(() => {
    const byId = (id: string | null) =>
      id ? allEngagements.find((e) => e.id === id) ?? null : null;
    return (
      byId(activeEngagementId) ??
      visibleEngagements[0] ??
      allEngagements[0] ??
      null
    );
  }, [activeEngagementId, visibleEngagements, allEngagements]);

  const projectsInActive = useMemo(() => {
    if (!activeProjectId) return projects;
    return projects.filter((p) => p.id === activeProjectId);
  }, [projects, activeProjectId]);

  // Load providers
  useEffect(() => {
    if (!token) {
      setProviders(null);
      return;
    }
    let cancelled = false;
    setProvidersLoading(true);
    setProvidersError(null);
    listProviders(token)
      .then((rows) => {
        if (!cancelled) setProviders(rows);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setProvidersError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setProvidersLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  // Load plugins (just for the count)
  useEffect(() => {
    if (!token) {
      setPlugins(null);
      return;
    }
    let cancelled = false;
    setPluginsLoading(true);
    setPluginsError(null);
    listInstalledPlugins(token)
      .then((rows) => {
        if (!cancelled) setPlugins(rows);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        setPluginsError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setPluginsLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  function handleSignOut() {
    setToken("");
    try {
      localStorage.removeItem("celerex_jwt");
    } catch {
      // ignore — non-browser env
    }
  }

  const defaultProvider = providers?.includes("default") ? "default" : null;

  return (
    <div
      data-testid="settings-page"
      style={{
        flex: 1,
        overflow: "auto",
        padding: "40px 24px",
        background: "var(--celerex-bg-primary)",
      }}
    >
      <div style={{ maxWidth: 720, margin: "0 auto" }}>
        {/* Header */}
        <div style={{ marginBottom: 28 }}>
          <h1
            style={{
              fontSize: 24,
              fontWeight: 700,
              color: "var(--celerex-text-primary)",
              margin: 0,
            }}
          >
            Settings
          </h1>
          <p
            style={{
              fontSize: 13,
              color: "var(--celerex-text-secondary)",
              margin: "4px 0 0",
            }}
          >
            Review what is configured and jump to the right surface to manage
            it.
          </p>
        </div>

        {/* Account / Engagement */}
        <Section title="Account / Engagement">
          <Row label="Active engagement">
            {effectiveEngagement ? (
              <div>
                <div
                  data-testid="settings-active-engagement-name"
                  style={{
                    fontSize: 13,
                    color: "var(--celerex-text-primary)",
                    fontWeight: 500,
                  }}
                >
                  {effectiveEngagement.name}
                </div>
                <div
                  style={{
                    fontFamily: "monospace",
                    fontSize: 11,
                    color: "var(--celerex-text-secondary)",
                    marginTop: 2,
                  }}
                >
                  {effectiveEngagement.id}
                </div>
              </div>
            ) : (
              <Muted>None selected</Muted>
            )}
          </Row>
          <Row label="Auth token">
            {token ? (
              <div
                style={{
                  fontSize: 12,
                  color: "var(--celerex-text-secondary)",
                  fontFamily: "monospace",
                }}
              >
                {redactToken(token)}
              </div>
            ) : (
              <Muted>Not signed in</Muted>
            )}
          </Row>
          <ActionRow>
            <Button
              kind="danger"
              disabled={!token}
              onClick={handleSignOut}
              testId="settings-sign-out"
            >
              Sign out
            </Button>
          </ActionRow>
        </Section>

        {/* LLM Providers */}
        <Section title="LLM Providers">
          {!token ? (
            <Muted>Sign in to load configured providers.</Muted>
          ) : providersLoading && providers === null ? (
            <Muted>Loading…</Muted>
          ) : providersError ? (
            <Banner kind="error">
              Failed to load providers: {providersError}
            </Banner>
          ) : !providers || providers.length === 0 ? (
            <Banner kind="warn">
              No providers configured. Set <code>ANTHROPIC_API_KEY</code>{" "}
              (or <code>OPENAI_API_KEY</code> / <code>MOONSHOT_API_KEY</code>{" "}
              / <code>GEMINI_API_KEY</code>) on the backend.
            </Banner>
          ) : (
            <div>
              <div
                data-testid="settings-providers"
                style={{
                  display: "flex",
                  flexWrap: "wrap",
                  gap: 6,
                  marginBottom: 8,
                }}
              >
                {providers.map((id) => (
                  <Badge key={id}>{id}</Badge>
                ))}
              </div>
              {defaultProvider && (
                <div
                  data-testid="settings-providers-default"
                  style={{
                    fontSize: 12,
                    color: "var(--celerex-text-secondary)",
                  }}
                >
                  Default: <code>{defaultProvider}</code>
                </div>
              )}
            </div>
          )}
        </Section>

        {/* Provider API Keys (per-engagement, encrypted) */}
        <Section title="Provider API Keys">
          <ProviderKeysSettings />
        </Section>

        {/* Devices (OmniVyx Design pairing) */}
        <Section title="Devices">
          <ApproveDeviceSettings />
        </Section>

        {/* Engagements & Projects */}
        <Section title="Engagements & Projects">
          <Row label="Engagements">
            <CountValue
              count={allEngagements.length}
              singular="engagement"
              plural="engagements"
              testId="settings-engagement-count"
            />
          </Row>
          <Row
            label={
              activeProjectId
                ? "Projects (active)"
                : "Projects in current engagement"
            }
          >
            <CountValue
              count={projectsInActive.length}
              singular="project"
              plural="projects"
              testId="settings-project-count"
            />
          </Row>
          <ActionRow>
            <Button
              onClick={() => setEngagementManagerOpen(true)}
              testId="settings-manage-engagements"
            >
              Manage engagements & projects
            </Button>
          </ActionRow>
        </Section>

        {/* Plugins */}
        <Section title="Plugins">
          <Row label="Installed">
            {!token ? (
              <Muted>Sign in to load plugins.</Muted>
            ) : pluginsLoading && plugins === null ? (
              <Muted>Loading…</Muted>
            ) : pluginsError ? (
              <Banner kind="error">
                Failed to load plugins: {pluginsError}
              </Banner>
            ) : (
              <CountValue
                count={plugins?.length ?? 0}
                singular="plugin"
                plural="plugins"
                testId="settings-plugin-count"
              />
            )}
          </Row>
          <ActionRow>
            <Button
              onClick={() => setActiveView("plugins")}
              testId="settings-manage-plugins"
            >
              Manage plugins
            </Button>
          </ActionRow>
        </Section>

        {/* About */}
        <Section title="About">
          <Row label="App">OmniVyx Agent</Row>
          <Row label="Version">
            <code
              style={{
                fontSize: 12,
                fontFamily: "monospace",
                color: "var(--celerex-text-secondary)",
              }}
            >
              {APP_VERSION}
            </code>
          </Row>
          <Row label="Backend base URL">
            <code
              style={{
                fontSize: 12,
                fontFamily: "monospace",
                color: "var(--celerex-text-secondary)",
                wordBreak: "break-all",
              }}
            >
              {API_BASE}
            </code>
          </Row>
        </Section>
      </div>

      <EngagementManager
        open={engagementManagerOpen}
        onClose={() => setEngagementManagerOpen(false)}
      />
    </div>
  );
}

/* -------------------------------------------------------------- presentational */

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 28 }}>
      <div
        style={{
          fontSize: 14,
          fontWeight: 600,
          color: "var(--celerex-text-primary)",
          marginBottom: 12,
          paddingBottom: 8,
          borderBottom: "1px solid var(--celerex-border)",
        }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function Row({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        padding: "8px 0",
        gap: 24,
      }}
    >
      <span
        style={{
          fontSize: 13,
          color: "var(--celerex-text-primary)",
          fontWeight: 400,
          flexShrink: 0,
        }}
      >
        {label}
      </span>
      <div style={{ textAlign: "right", maxWidth: "70%" }}>{children}</div>
    </div>
  );
}

function ActionRow({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        justifyContent: "flex-end",
        gap: 8,
        padding: "8px 0 2px",
      }}
    >
      {children}
    </div>
  );
}

function Muted({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontSize: 13,
        color: "var(--celerex-text-secondary)",
        fontStyle: "italic",
      }}
    >
      {children}
    </span>
  );
}

function Badge({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontSize: 12,
        fontFamily: "monospace",
        padding: "3px 8px",
        borderRadius: 12,
        background: "rgba(59, 130, 246, 0.10)",
        color: "var(--celerex-accent)",
        border: "1px solid rgba(59, 130, 246, 0.3)",
      }}
    >
      {children}
    </span>
  );
}

function Button({
  children,
  onClick,
  disabled,
  kind = "default",
  testId,
}: {
  children: React.ReactNode;
  onClick: () => void;
  disabled?: boolean;
  kind?: "default" | "danger";
  testId?: string;
}) {
  const isDanger = kind === "danger";
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      data-testid={testId}
      style={{
        padding: "6px 12px",
        borderRadius: 6,
        border: `1px solid ${
          isDanger ? "#dc2626" : "var(--celerex-border)"
        }`,
        background: isDanger ? "transparent" : "transparent",
        color: isDanger
          ? "#dc2626"
          : "var(--celerex-text-primary)",
        cursor: disabled ? "not-allowed" : "pointer",
        fontSize: 13,
        fontWeight: 500,
        opacity: disabled ? 0.5 : 1,
      }}
    >
      {children}
    </button>
  );
}

function CountValue({
  count,
  singular,
  plural,
  testId,
}: {
  count: number;
  singular: string;
  plural: string;
  testId?: string;
}) {
  return (
    <span
      data-testid={testId}
      style={{
        fontSize: 13,
        color: "var(--celerex-text-primary)",
      }}
    >
      {count} {count === 1 ? singular : plural}
    </span>
  );
}

function Banner({
  kind,
  children,
}: {
  kind: "warn" | "error";
  children: React.ReactNode;
}) {
  const palette =
    kind === "error"
      ? { bg: "rgba(220, 38, 38, 0.08)", fg: "#dc2626", border: "#dc2626" }
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
      }}
    >
      {children}
    </div>
  );
}

function redactToken(token: string): string {
  const trimmed = token.trim();
  if (trimmed.length <= 12) return "•".repeat(Math.max(4, trimmed.length));
  return `${trimmed.slice(0, 6)}…${trimmed.slice(-4)}`;
}
