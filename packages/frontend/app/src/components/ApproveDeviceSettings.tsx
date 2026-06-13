import { useState } from "react";
import { useAtomValue } from "jotai";
import { tokenAtom } from "../state/atoms";
import { approveDevice } from "../api/client";

/**
 * Approve a pending Celerex Design device-pairing request. The Design app shows
 * the user a short code (XXXX-XXXX) when it asks to connect; entering it here
 * (while signed in to this workspace) approves the pairing server-side. The
 * backend returns the device label on success, or 404 if the code expired.
 */
export function ApproveDeviceSettings() {
  const token = useAtomValue(tokenAtom);
  const [code, setCode] = useState("");
  const [status, setStatus] = useState<{ ok: boolean; msg: string } | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setBusy(true);
    setStatus(null);
    try {
      const r = await approveDevice(token, code.trim().toUpperCase());
      setStatus({ ok: true, msg: `Approved${r.label ? ` ${r.label}` : ""}.` });
      setCode("");
    } catch {
      setStatus({ ok: false, msg: "Code not found or expired." });
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <div style={{ fontSize: 13, color: "var(--celerex-text-secondary)", marginBottom: 2 }}>
        When Celerex Design asks to connect, it shows a short pairing code. Enter it here to
        approve the device for this workspace.
      </div>
      <div
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
          Pairing code
        </span>
        <input
          data-testid="approve-device-input"
          value={code}
          onChange={(e) => setCode(e.target.value.toUpperCase())}
          placeholder="XXXX-XXXX"
          style={{ ...inputStyle, flex: 1 }}
        />
        <button
          data-testid="approve-device-submit"
          onClick={() => void submit()}
          disabled={busy || !code.trim()}
          style={btnPrimary(busy)}
        >
          {busy ? "Approving…" : "Approve"}
        </button>
      </div>
      {status && (
        <div
          data-testid="approve-device-status"
          style={{
            padding: "8px 12px",
            borderRadius: 8,
            border: `1px solid ${status.ok ? "#166534" : "#7f1d1d"}`,
            background: status.ok ? "rgba(22,101,52,0.18)" : "rgba(127,29,29,0.18)",
            color: status.ok ? "#22c55e" : "#fca5a5",
            fontSize: 12.5,
          }}
        >
          {status.msg}
        </div>
      )}
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
