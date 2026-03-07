import { useState, useEffect, useRef, useCallback } from "react";

interface Peer {
  name: string;
  color: string;
}

const PEER_COLORS = ["#3b82f6", "#ef4444", "#10b981", "#f59e0b", "#8b5cf6", "#ec4899"];

export function useCollaboration(docId: string) {
  const [connected, setConnected] = useState(false);
  const [peers, setPeers] = useState<Peer[]>([]);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const connect = useCallback(() => {
    try {
      const ws = new WebSocket("ws://localhost:3001/ws");
      wsRef.current = ws;

      ws.onopen = () => {
        setConnected(true);
        ws.send(JSON.stringify({ type: "join", user: "You", docId }));
      };

      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          // Since the backend is an echo server, our own "join" message comes back.
          // Simulate receiving a peer from the echoed message.
          if (data.type === "join" && data.user) {
            setPeers((prev) => {
              // Avoid duplicates
              if (prev.some((p) => p.name === data.user)) return prev;
              const color = PEER_COLORS[prev.length % PEER_COLORS.length];
              return [...prev, { name: data.user, color }];
            });
          } else if (data.type === "leave" && data.user) {
            setPeers((prev) => prev.filter((p) => p.name !== data.user));
          }
        } catch {
          // Ignore non-JSON messages
        }
      };

      ws.onclose = () => {
        setConnected(false);
        wsRef.current = null;
        // Attempt reconnect after 5 seconds
        reconnectTimer.current = setTimeout(() => connect(), 5000);
      };

      ws.onerror = () => {
        // Silently handle errors - the onclose handler will fire after this
        ws.close();
      };
    } catch {
      // WebSocket constructor can throw if URL is invalid; fail silently
      setConnected(false);
    }
  }, [docId]);

  useEffect(() => {
    connect();

    return () => {
      if (reconnectTimer.current) {
        clearTimeout(reconnectTimer.current);
        reconnectTimer.current = null;
      }
      const ws = wsRef.current;
      if (ws) {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ type: "leave" }));
        }
        ws.close();
        wsRef.current = null;
      }
      setPeers([]);
    };
  }, [connect]);

  return { connected, peers };
}
