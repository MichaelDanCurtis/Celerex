import { useEffect, useState, useRef, useCallback } from "react";
import { startCopilotAuth, pollCopilotAuth } from "../api/client";

interface CopilotAuthDialogProps {
  open: boolean;
  onClose: () => void;
}

type Phase = "loading" | "awaiting" | "complete" | "error";

export function CopilotAuthDialog({ open, onClose }: CopilotAuthDialogProps) {
  const [phase, setPhase] = useState<Phase>("loading");
  const [userCode, setUserCode] = useState("");
  const [verificationUri, setVerificationUri] = useState("");
  const [deviceCode, setDeviceCode] = useState("");
  const [errorMsg, setErrorMsg] = useState("");
  const [copied, setCopied] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cleanup = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  }, []);

  // Start auth flow when dialog opens
  useEffect(() => {
    if (!open) {
      cleanup();
      return;
    }

    setPhase("loading");
    setCopied(false);

    startCopilotAuth()
      .then((data) => {
        setUserCode(data.user_code);
        setVerificationUri(data.verification_uri);
        setDeviceCode(data.device_code);
        setPhase("awaiting");
      })
      .catch((err) => {
        setErrorMsg(
          err instanceof Error ? err.message : "Failed to start authentication"
        );
        setPhase("error");
      });

    return cleanup;
  }, [open, cleanup]);

  // Poll for completion
  useEffect(() => {
    if (phase !== "awaiting" || !deviceCode) return;

    pollRef.current = setInterval(() => {
      pollCopilotAuth(deviceCode)
        .then((data) => {
          if (data.status === "complete") {
            setPhase("complete");
            if (pollRef.current) {
              clearInterval(pollRef.current);
              pollRef.current = null;
            }
            closeTimerRef.current = setTimeout(() => {
              onClose();
            }, 2000);
          }
        })
        .catch(() => {
          // Silently ignore poll errors — backend may be briefly unavailable
        });
    }, 5000);

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [phase, deviceCode, onClose]);

  // Escape key
  useEffect(() => {
    if (!open) return;
    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  if (!open) return null;

  function handleCopy() {
    navigator.clipboard.writeText(userCode).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0, 0, 0, 0.4)",
      }}
      onClick={onClose}
    >
      <div
        style={{
          width: "100%",
          maxWidth: 440,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          style={{
            padding: "16px 20px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 16,
              fontWeight: 600,
              color: "var(--celerex-text-primary, #111)",
            }}
          >
            Connect GitHub Copilot
          </h2>
          <button
            onClick={onClose}
            style={{
              border: "none",
              background: "none",
              cursor: "pointer",
              fontSize: 18,
              color: "var(--celerex-text-secondary, #888)",
              lineHeight: 1,
              padding: 4,
            }}
          >
            {"\u2715"}
          </button>
        </div>

        {/* Content */}
        <div style={{ padding: "24px 20px", textAlign: "center" }}>
          {phase === "loading" && (
            <div style={{ padding: "20px 0" }}>
              <div
                style={{
                  width: 32,
                  height: 32,
                  border: "3px solid var(--celerex-border, #e5e7eb)",
                  borderTopColor: "var(--celerex-accent, #6366f1)",
                  borderRadius: "50%",
                  animation: "copilot-spin 0.8s linear infinite",
                  margin: "0 auto 12px",
                }}
              />
              <p
                style={{
                  margin: 0,
                  fontSize: 13,
                  color: "var(--celerex-text-secondary, #888)",
                }}
              >
                Starting authentication...
              </p>
              <style>{`@keyframes copilot-spin { to { transform: rotate(360deg); } }`}</style>
            </div>
          )}

          {phase === "awaiting" && (
            <>
              <p
                style={{
                  margin: "0 0 16px",
                  fontSize: 14,
                  color: "var(--celerex-text-primary, #111)",
                }}
              >
                Enter this code at GitHub:
              </p>
              <div
                style={{
                  fontSize: 32,
                  fontWeight: 700,
                  fontFamily:
                    "ui-monospace, 'SF Mono', Monaco, 'Cascadia Code', monospace",
                  letterSpacing: "0.15em",
                  color: "var(--celerex-text-primary, #111)",
                  background: "var(--celerex-bg-secondary, #f5f5f5)",
                  border: "2px solid var(--celerex-border, #e5e7eb)",
                  borderRadius: 10,
                  padding: "16px 24px",
                  margin: "0 auto 20px",
                  display: "inline-block",
                  userSelect: "all",
                }}
              >
                {userCode}
              </div>
              <div
                style={{
                  display: "flex",
                  gap: 10,
                  justifyContent: "center",
                  marginBottom: 20,
                }}
              >
                <button
                  onClick={() =>
                    window.open(verificationUri, "_blank", "noopener")
                  }
                  style={{
                    padding: "8px 16px",
                    borderRadius: 6,
                    border: "none",
                    background: "var(--celerex-accent, #6366f1)",
                    color: "#fff",
                    cursor: "pointer",
                    fontSize: 13,
                    fontWeight: 600,
                  }}
                >
                  Open GitHub
                </button>
                <button
                  onClick={handleCopy}
                  style={{
                    padding: "8px 16px",
                    borderRadius: 6,
                    border: "1px solid var(--celerex-border, #e5e7eb)",
                    background: "var(--celerex-bg-secondary, #f5f5f5)",
                    color: "var(--celerex-text-primary, #111)",
                    cursor: "pointer",
                    fontSize: 13,
                    fontWeight: 500,
                  }}
                >
                  {copied ? "Copied!" : "Copy Code"}
                </button>
              </div>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 8,
                  color: "var(--celerex-text-secondary, #888)",
                  fontSize: 12,
                }}
              >
                <div
                  style={{
                    width: 14,
                    height: 14,
                    border: "2px solid var(--celerex-border, #e5e7eb)",
                    borderTopColor: "var(--celerex-accent, #6366f1)",
                    borderRadius: "50%",
                    animation: "copilot-spin 0.8s linear infinite",
                  }}
                />
                Waiting for authorization...
              </div>
              <style>{`@keyframes copilot-spin { to { transform: rotate(360deg); } }`}</style>
            </>
          )}

          {phase === "complete" && (
            <div style={{ padding: "20px 0" }}>
              <div
                style={{
                  fontSize: 40,
                  marginBottom: 12,
                }}
              >
                {"\u2705"}
              </div>
              <p
                style={{
                  margin: 0,
                  fontSize: 15,
                  fontWeight: 600,
                  color: "var(--celerex-text-primary, #111)",
                }}
              >
                GitHub Copilot connected successfully!
              </p>
              <p
                style={{
                  margin: "8px 0 0",
                  fontSize: 12,
                  color: "var(--celerex-text-secondary, #888)",
                }}
              >
                This dialog will close automatically...
              </p>
            </div>
          )}

          {phase === "error" && (
            <div style={{ padding: "20px 0" }}>
              <div
                style={{
                  fontSize: 40,
                  marginBottom: 12,
                }}
              >
                {"\u26A0\uFE0F"}
              </div>
              <p
                style={{
                  margin: "0 0 8px",
                  fontSize: 14,
                  fontWeight: 600,
                  color: "var(--celerex-text-primary, #111)",
                }}
              >
                Connection failed
              </p>
              <p
                style={{
                  margin: 0,
                  fontSize: 12,
                  color: "var(--celerex-text-secondary, #888)",
                  wordBreak: "break-word",
                }}
              >
                {errorMsg}
              </p>
              <button
                onClick={onClose}
                style={{
                  marginTop: 16,
                  padding: "8px 20px",
                  borderRadius: 6,
                  border: "1px solid var(--celerex-border, #e5e7eb)",
                  background: "var(--celerex-bg-secondary, #f5f5f5)",
                  color: "var(--celerex-text-primary, #111)",
                  cursor: "pointer",
                  fontSize: 13,
                  fontWeight: 500,
                }}
              >
                Close
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
