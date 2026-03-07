import { useState, useEffect, useRef, useCallback } from "react";

interface ShareDialogProps {
  open: boolean;
  onClose: () => void;
}

type Role = "Owner" | "Editor" | "Viewer";

interface Collaborator {
  id: string;
  name: string;
  email: string;
  role: Role;
  color: string;
}

const MOCK_COLLABORATORS: Collaborator[] = [
  { id: "1", name: "Alice Chen", email: "alice@celerex.io", role: "Owner", color: "#6366f1" },
  { id: "2", name: "Bob Martinez", email: "bob@celerex.io", role: "Editor", color: "#0ea5e9" },
  { id: "3", name: "Carol Wu", email: "carol@celerex.io", role: "Viewer", color: "#10b981" },
];

const MOCK_SHARE_URL = "https://app.celerex.io/share/abc123-mock";

function getInitials(name: string): string {
  return name
    .split(" ")
    .map((p) => p[0])
    .join("")
    .toUpperCase()
    .slice(0, 2);
}

const roleBadgeColors: Record<Role, { bg: string; text: string }> = {
  Owner: { bg: "rgba(99,102,241,0.15)", text: "#6366f1" },
  Editor: { bg: "rgba(14,165,233,0.15)", text: "#0ea5e9" },
  Viewer: { bg: "rgba(156,163,175,0.15)", text: "#6b7280" },
};

export function ShareDialog({ open, onClose }: ShareDialogProps) {
  const [collaborators, setCollaborators] = useState<Collaborator[]>(MOCK_COLLABORATORS);
  const [emailInput, setEmailInput] = useState("");
  const [copied, setCopied] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setEmailInput("");
    setCopied(false);
    requestAnimationFrame(() => inputRef.current?.focus());

    function handleKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  const handleInvite = useCallback(() => {
    const email = emailInput.trim();
    if (!email || !email.includes("@")) return;
    const namePart = email.split("@")[0];
    const displayName = namePart.charAt(0).toUpperCase() + namePart.slice(1);
    const colors = ["#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#14b8a6"];
    const color = colors[collaborators.length % colors.length];
    setCollaborators((prev) => [
      ...prev,
      {
        id: `inv-${Date.now()}`,
        name: displayName,
        email,
        role: "Viewer",
        color,
      },
    ]);
    setEmailInput("");
  }, [emailInput, collaborators.length]);

  const handleCopyLink = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(MOCK_SHARE_URL);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Fallback: silent fail in non-secure contexts
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, []);

  if (!open) return null;

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
          maxWidth: 480,
          background: "var(--celerex-bg-primary, #fff)",
          borderRadius: 12,
          boxShadow: "0 16px 48px rgba(0,0,0,0.2)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          maxHeight: "min(560px, 80vh)",
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
            Share Document
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

        {/* Invite input */}
        <div
          style={{
            padding: "16px 20px",
            borderBottom: "1px solid var(--celerex-border, #e5e7eb)",
            display: "flex",
            gap: 8,
          }}
        >
          <input
            ref={inputRef}
            type="email"
            placeholder="Invite by email address..."
            value={emailInput}
            onChange={(e) => setEmailInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                handleInvite();
              }
            }}
            style={{
              flex: 1,
              padding: "8px 12px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border, #d1d5db)",
              fontSize: 13,
              background: "var(--celerex-bg-secondary, #f9fafb)",
              color: "var(--celerex-text-primary, #111)",
              outline: "none",
            }}
          />
          <button
            onClick={handleInvite}
            disabled={!emailInput.trim() || !emailInput.includes("@")}
            style={{
              padding: "8px 16px",
              borderRadius: 6,
              border: "none",
              cursor: emailInput.trim() && emailInput.includes("@") ? "pointer" : "not-allowed",
              fontSize: 13,
              fontWeight: 500,
              background: "var(--celerex-accent, #6366f1)",
              color: "#fff",
              opacity: emailInput.trim() && emailInput.includes("@") ? 1 : 0.5,
            }}
          >
            Invite
          </button>
        </div>

        {/* Collaborators list */}
        <div style={{ overflowY: "auto", padding: "8px 20px 12px" }}>
          <h3
            style={{
              margin: "8px 0",
              fontSize: 11,
              fontWeight: 700,
              textTransform: "uppercase",
              letterSpacing: "0.05em",
              color: "var(--celerex-text-secondary, #888)",
            }}
          >
            Collaborators ({collaborators.length})
          </h3>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            {collaborators.map((c) => (
              <div
                key={c.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 12,
                  padding: "8px 0",
                }}
              >
                {/* Avatar */}
                <div
                  style={{
                    width: 32,
                    height: 32,
                    borderRadius: "50%",
                    background: c.color,
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    fontSize: 12,
                    fontWeight: 700,
                    color: "#fff",
                    flexShrink: 0,
                  }}
                >
                  {getInitials(c.name)}
                </div>

                {/* Name & email */}
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 13,
                      fontWeight: 500,
                      color: "var(--celerex-text-primary, #111)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {c.name}
                  </div>
                  <div
                    style={{
                      fontSize: 11,
                      color: "var(--celerex-text-secondary, #888)",
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {c.email}
                  </div>
                </div>

                {/* Role badge */}
                <span
                  style={{
                    padding: "3px 8px",
                    borderRadius: 4,
                    fontSize: 11,
                    fontWeight: 600,
                    background: roleBadgeColors[c.role].bg,
                    color: roleBadgeColors[c.role].text,
                    flexShrink: 0,
                  }}
                >
                  {c.role}
                </span>
              </div>
            ))}
          </div>
        </div>

        {/* Footer with Copy Link */}
        <div
          style={{
            padding: "12px 20px",
            borderTop: "1px solid var(--celerex-border, #e5e7eb)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <span
            style={{
              fontSize: 11,
              color: "var(--celerex-text-secondary, #888)",
            }}
          >
            Anyone with the link can view
          </span>
          <button
            onClick={handleCopyLink}
            style={{
              padding: "6px 14px",
              borderRadius: 6,
              border: "1px solid var(--celerex-border, #d1d5db)",
              cursor: "pointer",
              fontSize: 12,
              fontWeight: 500,
              background: copied ? "var(--celerex-accent, #6366f1)" : "transparent",
              color: copied ? "#fff" : "var(--celerex-text-primary, #111)",
              transition: "all 0.15s ease",
            }}
          >
            {copied ? "Copied!" : "Copy Link"}
          </button>
        </div>
      </div>
    </div>
  );
}
