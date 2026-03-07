import { useState, useRef, useEffect } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  AI_PROVIDERS,
  selectedProviderIdAtom,
  selectedModelAtom,
  apiKeysAtom,
  selectedProviderAtom,
  copilotAuthenticatedAtom,
} from "../state/aiProviderAtom";
import { getCopilotAuthStatus, fetchCopilotModels, type CopilotModel } from "../api/client";
import { CopilotAuthDialog } from "./CopilotAuthDialog";

export function AIProviderSelector() {
  const [open, setOpen] = useState(false);
  const [providerId, setProviderId] = useAtom(selectedProviderIdAtom);
  const [model, setModel] = useAtom(selectedModelAtom);
  const [apiKeys, setApiKeys] = useAtom(apiKeysAtom);
  const provider = useAtomValue(selectedProviderAtom);
  const [showKey, setShowKey] = useState(false);
  const [localKey, setLocalKey] = useState("");
  const popoverRef = useRef<HTMLDivElement>(null);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [copilotAuthenticated, setCopilotAuthenticated] = useAtom(copilotAuthenticatedAtom);
  const [copilotDialogOpen, setCopilotDialogOpen] = useState(false);
  const [copilotModels, setCopilotModels] = useState<CopilotModel[]>([]);
  const [copilotModelsLoading, setCopilotModelsLoading] = useState(false);

  // Check Copilot auth status on mount
  useEffect(() => {
    getCopilotAuthStatus()
      .then((data) => setCopilotAuthenticated(data.authenticated))
      .catch(() => {});
  }, [setCopilotAuthenticated]);

  // Fetch dynamic Copilot models when authenticated
  useEffect(() => {
    if (copilotAuthenticated && copilotModels.length === 0) {
      setCopilotModelsLoading(true);
      fetchCopilotModels()
        .then((models) => setCopilotModels(models))
        .catch(() => {})
        .finally(() => setCopilotModelsLoading(false));
    }
  }, [copilotAuthenticated, copilotModels.length]);

  // Sync local key input when provider changes or popover opens
  useEffect(() => {
    setLocalKey(apiKeys[providerId] || "");
    setShowKey(false);
  }, [providerId, open, apiKeys]);

  // Close popover on outside click
  useEffect(() => {
    if (!open) return;
    function handleClick(e: MouseEvent) {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node) &&
        buttonRef.current &&
        !buttonRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open]);

  function handleProviderChange(id: string) {
    setProviderId(id);
    localStorage.setItem("celerex-ai-provider", id);
    const p = AI_PROVIDERS.find((p) => p.id === id);
    if (p) {
      setModel(p.defaultModel);
      localStorage.setItem("celerex-ai-model", p.defaultModel);
    }
  }

  function handleModelChange(m: string) {
    setModel(m);
    localStorage.setItem("celerex-ai-model", m);
  }

  function handleSave() {
    const updated = { ...apiKeys, [providerId]: localKey };
    setApiKeys(updated);
    localStorage.setItem("celerex-ai-keys", JSON.stringify(updated));
    setOpen(false);
  }

  // Truncate model name for display
  const displayModel = model.length > 24 ? model.slice(0, 22) + "..." : model;

  return (
    <div style={{ position: "relative" }}>
      <button
        ref={buttonRef}
        onClick={() => setOpen((v) => !v)}
        style={{
          background: "var(--celerex-bg-secondary)",
          border: "1px solid var(--celerex-border)",
          borderRadius: 6,
          padding: "3px 8px",
          fontSize: 11,
          color: "var(--celerex-text-secondary)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          gap: 4,
          whiteSpace: "nowrap",
        }}
        title={`${provider.name} / ${model}`}
      >
        <span style={{ fontWeight: 500, color: "var(--celerex-text-primary)" }}>
          {provider.name}
        </span>
        <span style={{ opacity: 0.6 }}>/</span>
        <span>{displayModel}</span>
        <span style={{ fontSize: 9, marginLeft: 2 }}>{open ? "\u25B2" : "\u25BC"}</span>
      </button>

      {open && (
        <div
          ref={popoverRef}
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            right: 0,
            zIndex: 1000,
            background: "var(--celerex-bg-primary)",
            border: "1px solid var(--celerex-border)",
            borderRadius: 10,
            boxShadow: "0 8px 24px rgba(0,0,0,0.18)",
            width: 320,
            padding: 16,
            display: "flex",
            flexDirection: "column",
            gap: 14,
          }}
        >
          {/* Provider list */}
          <div>
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--celerex-text-secondary)",
                marginBottom: 6,
                textTransform: "uppercase",
                letterSpacing: 0.5,
              }}
            >
              Provider
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {AI_PROVIDERS.map((p) => (
                <button
                  key={p.id}
                  onClick={() => handleProviderChange(p.id)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    padding: "6px 8px",
                    borderRadius: 6,
                    border: "none",
                    background:
                      p.id === providerId
                        ? "var(--celerex-accent)"
                        : "transparent",
                    color:
                      p.id === providerId
                        ? "#fff"
                        : "var(--celerex-text-primary)",
                    cursor: "pointer",
                    fontSize: 12,
                    textAlign: "left",
                    width: "100%",
                    transition: "background 0.12s",
                  }}
                >
                  <span style={{ fontWeight: 500 }}>{p.name}</span>
                  <span
                    style={{
                      fontSize: 11,
                      opacity: 0.7,
                    }}
                  >
                    {p.description}
                  </span>
                </button>
              ))}
            </div>
          </div>

          {/* Model select */}
          <div>
            <div
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "var(--celerex-text-secondary)",
                marginBottom: 6,
                textTransform: "uppercase",
                letterSpacing: 0.5,
              }}
            >
              Model
            </div>
            {providerId === "github-copilot" && copilotAuthenticated && copilotModels.length > 0 ? (
              <select
                value={model}
                onChange={(e) => handleModelChange(e.target.value)}
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  borderRadius: 6,
                  border: "1px solid var(--celerex-border)",
                  background: "var(--celerex-bg-secondary)",
                  color: "var(--celerex-text-primary)",
                  fontSize: 12,
                  outline: "none",
                }}
              >
                {copilotModels.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name} ({m.vendor})
                  </option>
                ))}
              </select>
            ) : providerId === "github-copilot" && copilotModelsLoading ? (
              <div style={{ fontSize: 12, color: "var(--celerex-text-secondary)", padding: "6px 0" }}>
                Loading models...
              </div>
            ) : (
              <select
                value={model}
                onChange={(e) => handleModelChange(e.target.value)}
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  borderRadius: 6,
                  border: "1px solid var(--celerex-border)",
                  background: "var(--celerex-bg-secondary)",
                  color: "var(--celerex-text-primary)",
                  fontSize: 12,
                  outline: "none",
                }}
              >
                {provider.models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            )}
          </div>

          {/* API key or Copilot connect */}
          {providerId === "github-copilot" ? (
            <div>
              <div
                style={{
                  fontSize: 11,
                  fontWeight: 600,
                  color: "var(--celerex-text-secondary)",
                  marginBottom: 6,
                  textTransform: "uppercase",
                  letterSpacing: 0.5,
                }}
              >
                Authentication
              </div>
              {copilotAuthenticated ? (
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 8,
                    padding: "8px 12px",
                    borderRadius: 6,
                    background: "var(--celerex-bg-secondary)",
                    border: "1px solid var(--celerex-border)",
                    fontSize: 13,
                    color: "var(--celerex-text-primary)",
                    fontWeight: 500,
                  }}
                >
                  <span style={{ color: "#22c55e", fontSize: 16 }}>{"\u2713"}</span>
                  Connected
                </div>
              ) : (
                <button
                  onClick={() => setCopilotDialogOpen(true)}
                  style={{
                    width: "100%",
                    padding: "8px 0",
                    borderRadius: 6,
                    border: "none",
                    background: "var(--celerex-accent)",
                    color: "#fff",
                    cursor: "pointer",
                    fontSize: 13,
                    fontWeight: 600,
                  }}
                >
                  Connect GitHub Copilot
                </button>
              )}
            </div>
          ) : (
            <>
              <div>
                <div
                  style={{
                    fontSize: 11,
                    fontWeight: 600,
                    color: "var(--celerex-text-secondary)",
                    marginBottom: 6,
                    textTransform: "uppercase",
                    letterSpacing: 0.5,
                  }}
                >
                  API Key
                </div>
                <div style={{ display: "flex", gap: 4 }}>
                  <input
                    type={showKey ? "text" : "password"}
                    value={localKey}
                    onChange={(e) => setLocalKey(e.target.value)}
                    placeholder={`Enter ${provider.name} API key`}
                    style={{
                      flex: 1,
                      padding: "6px 8px",
                      borderRadius: 6,
                      border: "1px solid var(--celerex-border)",
                      background: "var(--celerex-bg-secondary)",
                      color: "var(--celerex-text-primary)",
                      fontSize: 12,
                      outline: "none",
                      fontFamily: "monospace",
                    }}
                  />
                  <button
                    onClick={() => setShowKey((v) => !v)}
                    style={{
                      padding: "4px 8px",
                      borderRadius: 6,
                      border: "1px solid var(--celerex-border)",
                      background: "var(--celerex-bg-secondary)",
                      color: "var(--celerex-text-secondary)",
                      cursor: "pointer",
                      fontSize: 11,
                    }}
                    title={showKey ? "Hide key" : "Show key"}
                  >
                    {showKey ? "Hide" : "Show"}
                  </button>
                </div>
              </div>

              {/* Save */}
              <button
                onClick={handleSave}
                style={{
                  padding: "7px 0",
                  borderRadius: 6,
                  border: "none",
                  background: "var(--celerex-accent)",
                  color: "#fff",
                  cursor: "pointer",
                  fontSize: 12,
                  fontWeight: 600,
                  transition: "opacity 0.12s",
                }}
              >
                Save
              </button>
            </>
          )}
        </div>
      )}
      <CopilotAuthDialog
        open={copilotDialogOpen}
        onClose={() => {
          setCopilotDialogOpen(false);
          // Re-check auth status after dialog closes
          getCopilotAuthStatus()
            .then((data) => setCopilotAuthenticated(data.authenticated))
            .catch(() => {});
        }}
      />
    </div>
  );
}
