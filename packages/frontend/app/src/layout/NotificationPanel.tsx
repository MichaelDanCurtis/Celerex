import { useEffect, useRef } from "react";
import { useAtomValue, useSetAtom } from "jotai";
import {
  notificationsAtom,
  markReadAtom,
  markAllReadAtom,
  unreadCountAtom,
} from "../state/notificationAtom";
import type { Notification } from "../state/notificationAtom";

const borderColorByType: Record<Notification["type"], string> = {
  info: "#3b82f6",
  success: "#22c55e",
  warning: "#f59e0b",
  error: "#ef4444",
};

function relativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp;
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function NotificationPanel({ onClose }: { onClose: () => void }) {
  const notifications = useAtomValue(notificationsAtom);
  const unreadCount = useAtomValue(unreadCountAtom);
  const markRead = useSetAtom(markReadAtom);
  const markAllRead = useSetAtom(markAllReadAtom);
  const panelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (panelRef.current && !panelRef.current.contains(e.target as Node)) {
        onClose();
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [onClose]);

  return (
    <div
      ref={panelRef}
      style={{
        position: "absolute",
        top: 44,
        right: 0,
        width: 320,
        maxHeight: 400,
        overflowY: "auto",
        background: "var(--celerex-bg-primary)",
        border: "1px solid var(--celerex-border)",
        borderRadius: 8,
        boxShadow: "0 8px 24px rgba(0,0,0,0.15)",
        zIndex: 1000,
      }}
    >
      {/* Header */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "10px 12px",
          borderBottom: "1px solid var(--celerex-border)",
        }}
      >
        <span
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: "var(--celerex-text-primary)",
          }}
        >
          Notifications
        </span>
        {unreadCount > 0 && (
          <button
            onClick={() => markAllRead()}
            style={{
              fontSize: 11,
              color: "var(--celerex-accent)",
              background: "none",
              border: "none",
              cursor: "pointer",
              padding: 0,
            }}
          >
            Mark all as read
          </button>
        )}
      </div>

      {/* List */}
      {notifications.length === 0 ? (
        <div
          style={{
            padding: 24,
            textAlign: "center",
            color: "var(--celerex-text-secondary)",
            fontSize: 13,
          }}
        >
          No notifications
        </div>
      ) : (
        notifications.map((notif) => (
          <div
            key={notif.id}
            onClick={() => {
              if (!notif.read) markRead(notif.id);
            }}
            style={{
              display: "flex",
              gap: 10,
              padding: "10px 12px",
              cursor: notif.read ? "default" : "pointer",
              background: notif.read
                ? "transparent"
                : "var(--celerex-bg-secondary, rgba(59,130,246,0.04))",
              borderBottom: "1px solid var(--celerex-border)",
              transition: "background 0.15s ease",
            }}
          >
            {/* Colored left border indicator */}
            <div
              style={{
                width: 3,
                minHeight: "100%",
                borderRadius: 2,
                background: borderColorByType[notif.type],
                flexShrink: 0,
              }}
            />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "baseline",
                  gap: 8,
                }}
              >
                <span
                  style={{
                    fontSize: 12,
                    fontWeight: notif.read ? 400 : 600,
                    color: "var(--celerex-text-primary)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {notif.title}
                </span>
                <span
                  style={{
                    fontSize: 10,
                    color: "var(--celerex-text-secondary)",
                    whiteSpace: "nowrap",
                    flexShrink: 0,
                  }}
                >
                  {relativeTime(notif.timestamp)}
                </span>
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--celerex-text-secondary)",
                  marginTop: 2,
                  lineHeight: 1.4,
                }}
              >
                {notif.message}
              </div>
            </div>
          </div>
        ))
      )}
    </div>
  );
}
