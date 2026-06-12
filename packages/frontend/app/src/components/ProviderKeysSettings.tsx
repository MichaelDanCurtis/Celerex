import { useCallback, useEffect, useState } from "react";
import { useAtomValue } from "jotai";
import { tokenAtom } from "../state/atoms";
import {
  listProviderKeys,
  setProviderKey,
  deleteProviderKey,
  type ApiProviderKey,
} from "../api/client";

// Mirrors SUPPORTED_PROVIDERS in the backend (providers/store.rs).
const PROVIDERS: { id: string; label: string; placeholder: string }[] = [
  { id: "anthropic", label: "Anthropic", placeholder: "sk-ant-…" },
  { id: "openai", label: "OpenAI", placeholder: "sk-…" },
  { id: "kimi", label: "Kimi (Moonshot)", placeholder: "sk-…" },
  { id: "gemini", label: "Google Gemini", placeholder: "AIza…" },
];

/**
 * Per-engagement LLM provider keys. The backend stores them encrypted and
 * injects them at call time for any flow node that names the provider
 * (EngagementLlmHandler); this is the UI that finally lets a workspace owner
 * set their own keys instead of relying on house env vars. Plaintext is sent
 * only on save and never returned — the list shows a masked preview.
 */
export function ProviderKeysSettings() {
  const token = useAtomValue(tokenAtom);
  const [keys, setKeys] = useState<Record<string, ApiProviderKey>>({});
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!token) return;
    try {
      const list = await listProviderKeys(token);
      const map: Record<string, ApiProviderKey> = {};
      for (const k of list) map[k.provider_id] = k;
      setKeys(map);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [token]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function save(providerId: string) {
    const key = (drafts[providerId] ?? "").trim();
    if (!key) return;
    setBusy(providerId);
    setError(null);
    try {
      await setProviderKey(token, providerId, key);
      setDrafts((d) => ({ ...d, [providerId]: "" }));
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function remove(providerId: string) {
    setBusy(providerId);
    setError(null);
    try {
      await deleteProviderKey(token, providerId);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ fontSize: 13, color: "var(--celerex-text-secondary)", marginBottom: 2 }}>
        Set your own API keys for this workspace. Flow nodes that name a provider use the
        workspace key; otherwise the house key (env) is used. Keys are stored encrypted and
        never shown again.
      </div>
      {error && (
        <div
          data-testid="provider-keys-error"
          style={{
            padding: "8px 12px",
            borderRadius: 8,
            border: "1px solid #7f1d1d",
            background: "rgba(127,29,29,0.18)",
            color: "#fca5a5",
            fontSize: 12.5,
          }}
        >
          {error}
        </div>
      )}
      {PROVIDERS.map((p) => {
        const existing = keys[p.id];
        const working = busy === p.id;
        return (
          <div
            key={p.id}
            data-testid={`provider-key-${p.id}`}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              padding: "10px 12px",
              borderRadius: 8,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary, #13161f)",
            }}
          >
            <span style={{ width: 130, fontSize: 13.5, fontWeight: 600, color: "var(--celerex-text-primary)" }}>
              {p.label}
            </span>
            {existing ? (
              <>
                <span
                  style={{
                    flex: 1,
                    fontSize: 12.5,
                    color: "#22c55e",
                    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
                  }}
                >
                  ✓ {existing.key_preview}
                </span>
                <input
                  type="password"
                  value={drafts[p.id] ?? ""}
                  onChange={(e) => setDrafts((d) => ({ ...d, [p.id]: e.target.value }))}
                  placeholder="Replace…"
                  style={inputStyle}
                />
                <button onClick={() => void save(p.id)} disabled={working || !(drafts[p.id] ?? "").trim()} style={btnPrimary(working)}>
                  Update
                </button>
                <button onClick={() => void remove(p.id)} disabled={working} style={btnGhost(working)}>
                  Remove
                </button>
              </>
            ) : (
              <>
                <input
                  type="password"
                  value={drafts[p.id] ?? ""}
                  onChange={(e) => setDrafts((d) => ({ ...d, [p.id]: e.target.value }))}
                  placeholder={p.placeholder}
                  style={{ ...inputStyle, flex: 1 }}
                />
                <button onClick={() => void save(p.id)} disabled={working || !(drafts[p.id] ?? "").trim()} style={btnPrimary(working)}>
                  {working ? "Saving…" : "Save"}
                </button>
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  padding: "6px 10px",
  borderRadius: 7,
  border: "1px solid var(--celerex-border)",
  background: "var(--celerex-bg-primary)",
  color: "var(--celerex-text-primary)",
  fontSize: 13,
  minWidth: 140,
};

function btnPrimary(busy: boolean): React.CSSProperties {
  return {
    padding: "6px 14px",
    borderRadius: 7,
    border: "none",
    background: "var(--celerex-accent)",
    color: "#fff",
    fontSize: 12.5,
    fontWeight: 600,
    cursor: busy ? "not-allowed" : "pointer",
    opacity: busy ? 0.6 : 1,
  };
}

function btnGhost(busy: boolean): React.CSSProperties {
  return {
    padding: "6px 12px",
    borderRadius: 7,
    border: "1px solid var(--celerex-border)",
    background: "transparent",
    color: "var(--celerex-text-secondary)",
    fontSize: 12.5,
    fontWeight: 600,
    cursor: busy ? "not-allowed" : "pointer",
  };
}
