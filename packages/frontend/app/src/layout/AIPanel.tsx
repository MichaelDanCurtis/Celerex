import { useState, useRef, useEffect, useCallback } from "react";
import { useAtom, useAtomValue } from "jotai";
import {
  chatMessagesAtom,
  editorStoreAtom,
  tokenAtom,
  genId,
  type ChatMessage,
} from "../state/atoms";
import {
  assistantChat,
  listProviders,
  type AssistantChatResponse,
} from "../api/client";
import {
  selectedModelAtom,
} from "../state/aiProviderAtom";
import { AIProviderSelector } from "./AIProviderSelector";
import { insertContentIntoDoc, extractDocText } from "../editor/insertContent";

// Extract reply text from the provider's raw response body.
// /api/assistant/chat returns the provider's raw response untouched, so we
// have to handle each shape (Anthropic, OpenAI, Gemini) we expect to see.
function extractReply(resp: AssistantChatResponse): string {
  const raw = resp.raw as any;
  // Anthropic: { content: [{ type: "text", text: "..." }], stop_reason: ... }
  if (raw?.content?.[0]?.text) return raw.content[0].text;
  // OpenAI: { choices: [{ message: { content: "..." } }] }
  if (raw?.choices?.[0]?.message?.content) return raw.choices[0].message.content;
  // Gemini: { candidates: [{ content: { parts: [{ text: "..." }] } }] }
  if (raw?.candidates?.[0]?.content?.parts?.[0]?.text) {
    return raw.candidates[0].content.parts[0].text;
  }
  return "(no reply)";
}

function formatTime(timestamp: number): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
  });
}

interface MessageBubbleProps {
  message: ChatMessage;
  onInsert?: (content: string) => void;
  onReplace?: (content: string) => void;
}

function MessageBubble({ message, onInsert, onReplace }: MessageBubbleProps) {
  const [inserted, setInserted] = useState(false);

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
  const isAssistant = message.role === "assistant";

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
      {/* Action buttons for assistant messages */}
      {isAssistant && (
        <div
          style={{
            display: "flex",
            gap: 6,
            marginTop: 4,
            flexWrap: "wrap",
          }}
        >
          <button
            onClick={() => {
              onInsert?.(message.content);
              setInserted(true);
              setTimeout(() => setInserted(false), 2000);
            }}
            style={{
              padding: "2px 8px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: inserted ? "var(--celerex-accent)" : "var(--celerex-bg-secondary)",
              color: inserted ? "#fff" : "var(--celerex-text-secondary)",
              cursor: "pointer",
              fontSize: 11,
              transition: "all 0.15s",
            }}
          >
            {inserted ? "Inserted!" : "Insert into doc"}
          </button>
          <button
            onClick={() => onReplace?.(message.content)}
            style={{
              padding: "2px 8px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-secondary)",
              cursor: "pointer",
              fontSize: 11,
            }}
          >
            Replace doc
          </button>
          <button
            onClick={() => navigator.clipboard.writeText(message.content)}
            style={{
              padding: "2px 8px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-secondary)",
              cursor: "pointer",
              fontSize: 11,
            }}
          >
            Copy
          </button>
        </div>
      )}
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

// Quick actions that read/edit the document
const QUICK_ACTIONS = [
  { id: "summarize", label: "Summarize", prompt: "Summarize the following document concisely:\n\n" },
  { id: "improve", label: "Improve", prompt: "Improve the writing quality, clarity, and flow of the following text. Return the improved version:\n\n" },
  { id: "grammar", label: "Fix Grammar", prompt: "Fix any grammar, spelling, and punctuation errors in the following text. Return the corrected version:\n\n" },
  { id: "continue", label: "Continue", prompt: "Continue writing from where this document leaves off, maintaining the same style and tone:\n\n" },
  { id: "shorter", label: "Make Shorter", prompt: "Make the following text more concise while preserving the key points. Return the shortened version:\n\n" },
  { id: "expand", label: "Expand", prompt: "Expand on the following text with more detail, examples, and depth:\n\n" },
];

export function AIPanel() {
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [messages, setMessages] = useAtom(chatMessagesAtom);
  const editorStore = useAtomValue(editorStoreAtom);
  const selectedModel = useAtomValue(selectedModelAtom);
  const token = useAtomValue(tokenAtom);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Backend-configured providers (from /api/providers). Empty = backend
  // hasn't been queried yet (or has no configured providers); falling back
  // to undefined lets /api/assistant/chat use its "default" provider.
  const [backendProviders, setBackendProviders] = useState<string[]>([]);
  const [backendProviderId, setBackendProviderId] = useState<string | undefined>(undefined);
  const [modelOverride, setModelOverride] = useState<string>("");

  // Load backend providers once we have a token. Failures are ignored — the
  // backend will use "default" when no provider_id is sent.
  useEffect(() => {
    if (!token) return;
    let cancelled = false;
    listProviders(token)
      .then((list) => {
        if (cancelled) return;
        setBackendProviders(list);
        if (list.length > 0) {
          setBackendProviderId((cur) => cur ?? list[0]);
        }
      })
      .catch(() => {
        // Leave backendProviderId undefined; backend falls back to "default".
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const getDocContext = useCallback(() => {
    if (!editorStore) return undefined;
    try {
      const text = extractDocText(editorStore);
      return text ? text.slice(0, 2000) : undefined;
    } catch {
      return undefined;
    }
  }, [editorStore]);

  const sendMessage = useCallback(async (userText: string, systemPrompt?: string) => {
    if (loading) return;

    const displayText = systemPrompt ? `[${userText}]` : userText;
    const actualMessage = systemPrompt
      ? systemPrompt + (getDocContext() || "(empty document)")
      : userText;

    const userMessage: ChatMessage = {
      id: genId("msg"),
      role: "user",
      content: displayText,
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage]);
    setInput("");
    setLoading(true);
    textareaRef.current?.focus();

    // Build context for non-quick-action messages
    const docContext = systemPrompt ? undefined : getDocContext();

    try {
      // Fold doc context (when not a quick action) into the user message so
      // the backend assistant sees it.
      const promptWithContext = docContext
        ? `${actualMessage}\n\n--- Document context ---\n${docContext}`
        : actualMessage;

      // Build a single-turn message array. The backend assistant treats this
      // as the full conversation, so we don't replay history for now — the
      // existing UX is one-shot per send.
      const apiMessages = [{ role: "user", content: promptWithContext }];

      const providerArg = backendProviderId || undefined;
      const modelArg = modelOverride.trim() ? modelOverride.trim() : (selectedModel || undefined);

      const resp = await assistantChat(token, apiMessages, providerArg, modelArg);
      const reply = extractReply(resp);
      const assistantMessage: ChatMessage = {
        id: genId("msg"),
        role: "assistant",
        content: reply,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, assistantMessage]);
    } catch (err) {
      const detail = err instanceof Error ? err.message : String(err);
      const errorMessage: ChatMessage = {
        id: genId("msg"),
        role: "assistant",
        content: `Error: ${detail}`,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, errorMessage]);
    } finally {
      setLoading(false);
    }
  }, [loading, getDocContext, setMessages, token, backendProviderId, modelOverride, selectedModel]);

  const handleSend = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed) return;
    sendMessage(trimmed);
  }, [input, sendMessage]);

  const handleQuickAction = useCallback((action: typeof QUICK_ACTIONS[number]) => {
    sendMessage(action.label, action.prompt);
  }, [sendMessage]);

  const handleInsert = useCallback((content: string) => {
    if (!editorStore) return;
    insertContentIntoDoc(editorStore, content);
  }, [editorStore]);

  const handleReplace = useCallback((content: string) => {
    if (!editorStore) return;
    // Clear existing content, then insert
    const root = editorStore.root;
    if (!root) return;
    const noteBlock = root.children?.find((c: any) => c.flavour === "affine:note");
    if (!noteBlock) return;

    // Remove existing paragraph/list/code children from note
    const childIds = [...(noteBlock.children || [])].map((c: any) => c.id);
    for (const id of childIds) {
      try { editorStore.deleteBlock(id); } catch { /* ignore */ }
    }

    // Insert new content
    insertContentIntoDoc(editorStore, content);
  }, [editorStore]);

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
        <span style={{ flex: 1 }} />
        <AIProviderSelector />
      </div>

      {/* Backend provider + model override row */}
      <div
        style={{
          display: "flex",
          gap: 6,
          alignItems: "center",
          padding: "6px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontSize: 11,
        }}
      >
        <span style={{ color: "var(--celerex-text-secondary)" }}>Backend:</span>
        <select
          value={backendProviderId ?? ""}
          onChange={(e) => setBackendProviderId(e.target.value || undefined)}
          disabled={backendProviders.length === 0}
          title={
            backendProviders.length === 0
              ? "No providers configured on backend — using default"
              : "Backend-configured providers from /api/providers"
          }
          style={{
            padding: "3px 6px",
            borderRadius: 4,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-bg-secondary)",
            color: "var(--celerex-text-primary)",
            fontSize: 11,
            outline: "none",
          }}
        >
          {backendProviders.length === 0 ? (
            <option value="">default</option>
          ) : (
            backendProviders.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))
          )}
        </select>
        <input
          type="text"
          value={modelOverride}
          onChange={(e) => setModelOverride(e.target.value)}
          placeholder="model (optional)"
          style={{
            flex: 1,
            minWidth: 0,
            padding: "3px 6px",
            borderRadius: 4,
            border: "1px solid var(--celerex-border)",
            background: "var(--celerex-bg-secondary)",
            color: "var(--celerex-text-primary)",
            fontSize: 11,
            outline: "none",
            fontFamily: "monospace",
          }}
        />
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
          <MessageBubble
            key={msg.id}
            message={msg}
            onInsert={handleInsert}
            onReplace={handleReplace}
          />
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

      {/* Quick Actions */}
      <div
        style={{
          padding: "6px 12px",
          borderTop: "1px solid var(--celerex-border)",
          display: "flex",
          gap: 4,
          flexWrap: "wrap",
        }}
      >
        {QUICK_ACTIONS.map((action) => (
          <button
            key={action.id}
            onClick={() => handleQuickAction(action)}
            disabled={loading}
            style={{
              padding: "3px 8px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: loading ? "var(--celerex-text-secondary)" : "var(--celerex-text-primary)",
              cursor: loading ? "not-allowed" : "pointer",
              fontSize: 11,
              fontWeight: 500,
              opacity: loading ? 0.5 : 1,
              transition: "opacity 0.15s",
            }}
          >
            {action.label}
          </button>
        ))}
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
