import { useCollaboration } from "../hooks/useCollaboration";

export function PresenceIndicator({ docId }: { docId: string }) {
  const { connected, peers } = useCollaboration(docId);

  if (!connected) {
    return (
      <div
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          fontSize: 11,
          color: "#9ca3af",
          opacity: 0.6,
          userSelect: "none",
        }}
        title="Collaboration offline"
      >
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: "#9ca3af",
          }}
        />
        Offline
      </div>
    );
  }

  // Always show "You" plus any peers
  const allUsers = [{ name: "You", color: "#22c55e" }, ...peers.filter((p) => p.name !== "You")];

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 2,
        userSelect: "none",
      }}
      title={`${allUsers.length} user${allUsers.length !== 1 ? "s" : ""} online: ${allUsers.map((u) => u.name).join(", ")}`}
    >
      {allUsers.map((user) => (
        <div
          key={user.name}
          style={{
            width: 22,
            height: 22,
            borderRadius: "50%",
            background: user.color,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 10,
            fontWeight: 600,
            color: "#fff",
            border: "2px solid var(--celerex-bg-primary)",
            marginLeft: -4,
            position: "relative",
            zIndex: allUsers.indexOf(user),
          }}
        >
          {user.name.charAt(0).toUpperCase()}
        </div>
      ))}
    </div>
  );
}
