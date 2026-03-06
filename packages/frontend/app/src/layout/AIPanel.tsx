import { useState, useRef, useEffect, useCallback } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  chatMessagesAtom,
  activeDocAtom,
  editorStoreAtom,
  genId,
  type ChatMessage,
} from "../state/atoms";
import { sendChatMessage } from "../api/client";

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
}

function MessageBubble({ message }: { message: ChatMessage }) {
  if (message.role === "system") {
    return (
      <div
        style={{
          alignSelf: "center",
          background: "var(--celerex-bg-secondary)",
          borderRadius: 8,
          padding: 12,
          fontSize: 13,
          lineHeight: 1.5,
          color: "var(--celerex-text-secondary)",
          textAlign: "center",
          maxWidth: "90%",
          whiteSpace: "pre-wrap",
        }}
      >
        {message.content}
        <div style={{ fontSize: 11, marginTop: 6, opacity: 0.6 }}>
          {formatTime(message.timestamp)}
        </div>
      </div>
    );
  }

  const isUser = message.role === "user";

  return (
    <div
      style={{
        alignSelf: isUser ? "flex-end" : "flex-start",
        maxWidth: "85%",
      }}
    >
      <div
        style={{
          background: isUser ? "var(--celerex-accent)" : "var(--celerex-bg-secondary)",
          color: isUser ? "#fff" : "var(--celerex-text-primary)",
          borderRadius: 8,
          padding: "8px 12px",
          fontSize: 13,
          lineHeight: 1.5,
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
        }}
      >
        {message.content}
      </div>
      <div
        style={{
          fontSize: 11,
          color: "var(--celerex-text-secondary)",
          marginTop: 4,
          textAlign: isUser ? "right" : "left",
          opacity: 0.6,
        }}
      >
        {formatTime(message.timestamp)}
      </div>
    </div>
  );
}

export function AIPanel() {
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [messages, setMessages] = useAtom(chatMessagesAtom);
  const activeDoc = useAtomValue(activeDocAtom);
  const editorStore = useAtomValue(editorStoreAtom);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSend = useCallback(async () => {
    const trimmed = input.trim();
    if (!trimmed || loading) return;

    const userMessage: ChatMessage = {
      id: genId("msg"),
      role: "user",
      content: trimmed,
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setLoading(true);

    // Focus back on textarea
    textareaRef.current?.focus();

    // Build document context from the editor store
    let docContext: string | undefined;
    try {
      if (editorStore) {
        const root = editorStore.document;
        if (root && Array.isArray(root)) {
          const textParts: string[] = [];
          for (const block of root) {
            if (block.content && Array.isArray(block.content)) {
              for (const inline of block.content) {
                if (typeof inline === "string") {
                  textParts.push(inline);
                } else if (inline?.text) {
                  textParts.push(inline.text);
                }
              }
            }
          }
          const fullText = textParts.join(" ").trim();
          if (fullText) {
            docContext = fullText.slice(0, 500);
          }
        }
      }
    } catch {
      // Ignore context extraction errors
    }

    try {
      const reply = await sendChatMessage(trimmed, docContext);
      const assistantMessage: ChatMessage = {
        id: genId("msg"),
        role: "assistant",
        content: reply,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMessage]);
    } catch {
      // Fall back to mock response when backend is unavailable
      const docTitle = activeDoc?.title ?? "your document";
      const assistantMessage: ChatMessage = {
        id: genId("msg"),
        role: "assistant",
        content: `I'm not connected to an AI backend yet, but I can see you're working on "${docTitle}". This is where contextual AI assistance will appear once the backend is running.`,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMessage]);
    } finally {
      setLoading(false);
    }
  }, [input, loading, activeDoc, editorStore, setMessages]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  const isDisabled = input.trim().length === 0 || loading;

  return (
    <aside
      style={{
        width: "var(--celerex-ai-panel-width)",
        height: "100%",
        borderLeft: "1px solid var(--celerex-border)",
        display: "flex",
        flexDirection: "column",
        background: "var(--celerex-bg-primary)",
      }}
    >
      {/* Header */}
      <div
        style={{
          padding: "12px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontWeight: 600,
          fontSize: 14,
          display: "flex",
          alignItems: "center",
          gap: 8,
        }}
      >
        <span style={{ fontSize: 16 }}>AI</span>
        <span style={{ color: "var(--celerex-text-secondary)", fontWeight: 400 }}>
          Co-Pilot
        </span>
      </div>

      {/* Messages Area */}
      <div
        style={{
          flex: 1,
          padding: 16,
          overflow: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 12,
        }}
      >
        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}
        {loading && (
          <div
            style={{
              alignSelf: "flex-start",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-secondary)",
              borderRadius: 8,
              padding: "8px 12px",
              fontSize: 13,
              lineHeight: 1.5,
            }}
          >
            ...
          </div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* Input */}
      <div
        style={{
          padding: 12,
          borderTop: "1px solid var(--celerex-border)",
        }}
      >
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "flex-end",
          }}
        >
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask anything about your workspace..."
            rows={2}
            style={{
              flex: 1,
              resize: "none",
              border: "1px solid var(--celerex-border)",
              borderRadius: 8,
              padding: "8px 12px",
              fontSize: 13,
              fontFamily: "inherit",
              outline: "none",
              background: "var(--celerex-bg-primary)",
              color: "var(--celerex-text-primary)",
            }}
          />
          <button
            disabled={isDisabled}
            onClick={handleSend}
            style={{
              padding: "8px 16px",
              borderRadius: 8,
              border: "none",
              background: isDisabled ? "var(--celerex-border)" : "var(--celerex-accent)",
              color: "#fff",
              cursor: isDisabled ? "not-allowed" : "pointer",
              fontSize: 13,
              fontWeight: 500,
              whiteSpace: "nowrap",
              opacity: isDisabled ? 0.6 : 1,
              transition: "background 0.15s, opacity 0.15s",
            }}
          >
            Send
          </button>
        </div>
      </div>
    </aside>
  );
}
