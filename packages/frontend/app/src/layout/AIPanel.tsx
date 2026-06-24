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
  assistantChatStream,
  listProviders,
  startCopilotLogin,
  getCopilotStatus,
} from "../api/client";
import {
  selectedModelAtom,
} from "../state/aiProviderAtom";
import { AIProviderSelector } from "./AIProviderSelector";
import { insertContentIntoDoc, extractDocText } from "../editor/insertContent";

// Note: the non-streaming `assistantChat` and its `AssistantChatResponse`
// shape remain exported from `../api/client` for any other caller that still
// needs a single-shot reply. AIPanel itself only uses the streaming variant.

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
  // AbortController for the in-flight streaming request. Stored in a ref so
  // the Stop button can reach it without re-rendering, and so that swapping
  // streams (rare) tears down the old fetch.
  const abortRef = useRef<AbortController | null>(null);

  // Backend-configured providers (from /api/providers). Empty = backend
  // hasn't been queried yet (or has no configured providers); falling back
  // to undefined lets /api/assistant/chat use its "default" provider.
  const [backendProviders, setBackendProviders] = useState<string[]>([]);
  const [backendProviderId, setBackendProviderId] = useState<string | undefined>(undefined);
  const [modelOverride, setModelOverride] = useState<string>("");

  // GitHub Copilot authorize-as-provider (device flow). Once connected, "copilot"
  // becomes a selectable provider that the Co-Pilot sends to /api/assistant/chat.
  const [copilotConnected, setCopilotConnected] = useState(false);
  const [copilotCode, setCopilotCode] = useState<string | null>(null);
  const [copilotVerifyUrl, setCopilotVerifyUrl] = useState<string | null>(null);
  const [copilotBusy, setCopilotBusy] = useState(false);
  const [copilotError, setCopilotError] = useState<string | null>(null);

  // Reflect an already-authorized Copilot on load so it can be selected.
  useEffect(() => {
    if (!token) return;
    let cancelled = false;
    getCopilotStatus(token)
      .then((s) => {
        if (cancelled) return;
        const connected = s.connected || s.status === "connected";
        setCopilotConnected(connected);
        // If Copilot is already authorized and nothing else is selected, default
        // the Co-Pilot to it so a reload doesn't silently fall back to a provider
        // with no key. (The backend "house" list is often empty per-engagement.)
        if (connected) setBackendProviderId((cur) => cur ?? "copilot");
      })
      .catch(() => {
        /* status hiccup — leave the Connect button available */
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  const connectCopilot = useCallback(async () => {
    if (!token) return;
    setCopilotBusy(true);
    setCopilotError(null);
    setCopilotCode(null);
    try {
      const start = await startCopilotLogin(token);
      setCopilotCode(start.user_code);
      setCopilotVerifyUrl(start.verification_uri);
      const delay = start.interval > 0 ? start.interval * 1000 : 2500;
      for (let i = 0; i < 200; i++) {
        const s = await getCopilotStatus(token);
        if (s.connected || s.status === "connected") {
          setCopilotConnected(true);
          setCopilotCode(null);
          setBackendProviderId("copilot");
          break;
        }
        if (s.status === "denied" || s.status === "expired" || s.status === "error") {
          setCopilotError(`GitHub login ${s.status}.`);
          break;
        }
        await new Promise((r) => setTimeout(r, delay));
      }
    } catch (e) {
      setCopilotError(e instanceof Error ? e.message : String(e));
    } finally {
      setCopilotBusy(false);
    }
  }, [token]);

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

    // Pre-allocate the assistant bubble that will accumulate streamed deltas.
    const assistantId = genId("msg");
    const assistantMessage: ChatMessage = {
      id: assistantId,
      role: "assistant",
      content: "",
      timestamp: Date.now(),
    };

    setMessages((prev) => [...prev, userMessage, assistantMessage]);
    setInput("");
    setLoading(true);
    textareaRef.current?.focus();

    // Build context for non-quick-action messages
    const docContext = systemPrompt ? undefined : getDocContext();

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
    // GitHub Copilot uses OpenAI-style model slugs; default to gpt-4o so the
    // Claude default doesn't get sent to Copilot (which would reject it).
    const isCopilot = backendProviderId === "copilot";
    const modelArg = modelOverride.trim()
      ? modelOverride.trim()
      : isCopilot
        ? "gpt-4o"
        : (selectedModel || undefined);

    // Tear down any prior in-flight stream and arm a new abort controller for
    // the Stop button.
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    let appended = "";
    const appendDelta = (chunk: string) => {
      appended += chunk;
      setMessages((prev) =>
        prev.map((m) =>
          m.id === assistantId ? { ...m, content: appended } : m,
        ),
      );
    };

    await assistantChatStream(
      token,
      apiMessages,
      providerArg,
      modelArg,
      {
        signal: controller.signal,
        onDelta: appendDelta,
        onDone: () => {
          // Finalize: nothing extra to do — bubble already holds full text.
        },
        onError: (msg) => {
          // Replace the streaming bubble with an error placeholder so the
          // user sees what went wrong instead of an empty assistant message.
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId
                ? { ...m, content: `Error: ${msg}` }
                : m,
            ),
          );
        },
      },
    );

    if (abortRef.current === controller) abortRef.current = null;
    setLoading(false);
  }, [loading, getDocContext, setMessages, token, backendProviderId, modelOverride, selectedModel]);

  const handleStop = useCallback(() => {
    abortRef.current?.abort();
    abortRef.current = null;
    setLoading(false);
  }, []);

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

  // Providers the Co-Pilot can send: the backend's list plus GitHub Copilot
  // once it's authorized for this workspace.
  const providerOptions = Array.from(
    new Set([...backendProviders, ...(copilotConnected ? ["copilot"] : [])]),
  );

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
          disabled={providerOptions.length === 0}
          title={
            providerOptions.length === 0
              ? "No providers configured — using default"
              : "Provider the Co-Pilot sends to the assistant"
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
          {providerOptions.length === 0 ? (
            <option value="">default</option>
          ) : (
            providerOptions.map((p) => (
              <option key={p} value={p}>
                {p === "copilot" ? "GitHub Copilot" : p}
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

      {/* GitHub Copilot: authorize as an AI provider for this workspace */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: 8,
          alignItems: "center",
          padding: "6px 16px",
          borderBottom: "1px solid var(--celerex-border)",
          fontSize: 11,
        }}
      >
        <span style={{ color: "var(--celerex-text-secondary)" }}>GitHub Copilot:</span>
        {copilotConnected ? (
          <span style={{ color: "#22c55e", fontWeight: 600 }}>✓ authorized</span>
        ) : (
          <button
            type="button"
            onClick={() => void connectCopilot()}
            disabled={copilotBusy || !token}
            style={{
              padding: "3px 8px",
              borderRadius: 4,
              border: "1px solid var(--celerex-border)",
              background: "var(--celerex-bg-secondary)",
              color: "var(--celerex-text-primary)",
              fontSize: 11,
              cursor: copilotBusy ? "wait" : "pointer",
              opacity: copilotBusy ? 0.6 : 1,
            }}
          >
            {copilotBusy ? "Waiting for GitHub…" : "Connect GitHub Copilot"}
          </button>
        )}
        {copilotCode ? (
          <span style={{ color: "var(--celerex-text-secondary)" }}>
            Go to{" "}
            <a href={copilotVerifyUrl ?? "https://github.com/login/device"} target="_blank" rel="noreferrer" style={{ color: "var(--celerex-accent)" }}>
              github.com/login/device
            </a>{" "}
            and enter <strong style={{ color: "var(--celerex-text-primary)" }}>{copilotCode}</strong>
          </span>
        ) : null}
        {copilotError ? <span style={{ color: "#f87171" }}>{copilotError}</span> : null}
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
          {loading ? (
            <button
              onClick={handleStop}
              style={{
                padding: "8px 16px",
                borderRadius: 8,
                border: "1px solid var(--celerex-border)",
                background: "var(--celerex-bg-secondary)",
                color: "var(--celerex-text-primary)",
                cursor: "pointer",
                fontSize: 13,
                fontWeight: 500,
                whiteSpace: "nowrap",
                transition: "background 0.15s, opacity 0.15s",
              }}
            >
              Stop
            </button>
          ) : (
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
          )}
        </div>
      </div>
    </aside>
  );
}
