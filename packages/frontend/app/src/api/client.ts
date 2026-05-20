// ---------------------------------------------------------------------------
// Celerex Frontend – API client for the Rust backend
// ---------------------------------------------------------------------------

export const API_BASE = "http://localhost:3001/api";

// ---- Response types matching the Rust API --------------------------------

export interface ApiDoc {
  id: string;
  title: string;
  engagement_id: string;
  created_at: string;
}

export interface ApiEngagement {
  id: string;
  name: string;
  created_at: string;
  docs: ApiDoc[];
}

interface FetchEngagementsResponse {
  engagements: ApiEngagement[];
}

// ---- Helpers -------------------------------------------------------------

async function request<T>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
    ...options,
  });

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(
      `API error ${res.status} ${res.statusText}${body ? `: ${body}` : ""}`,
    );
  }

  // DELETE endpoints may return 204 No Content
  if (res.status === 204 || res.headers.get("content-length") === "0") {
    return undefined as unknown as T;
  }

  return res.json() as Promise<T>;
}

// ---- Auth endpoints ------------------------------------------------------

export interface LoginResponse {
  token: string;
  engagement: { id: string; name: string };
}

/**
 * Dev-mode signin: swaps an engagement name for a JWT minted by the backend.
 * No password — this stands in for the missing real auth flow.
 */
export async function login(engagementName: string): Promise<LoginResponse> {
  return request<LoginResponse>("/auth/login", {
    method: "POST",
    body: JSON.stringify({ engagement_name: engagementName }),
  });
}

// ---- Engagement endpoints ------------------------------------------------

export async function fetchEngagements(): Promise<ApiEngagement[]> {
  const data = await request<FetchEngagementsResponse>("/engagements");
  return data.engagements;
}

export async function createEngagement(name: string): Promise<ApiEngagement> {
  return request<ApiEngagement>("/engagements", {
    method: "POST",
    body: JSON.stringify({ name }),
  });
}

export async function deleteEngagement(id: string): Promise<void> {
  await request<void>(`/engagements/${id}`, {
    method: "DELETE",
  });
}

// ---- Doc endpoints -------------------------------------------------------

export async function createDoc(
  engagementId: string,
  title: string,
): Promise<ApiDoc> {
  return request<ApiDoc>(`/engagements/${engagementId}/docs`, {
    method: "POST",
    body: JSON.stringify({ title }),
  });
}

export async function updateDocTitle(
  engagementId: string,
  docId: string,
  title: string,
): Promise<ApiDoc> {
  return request<ApiDoc>(`/engagements/${engagementId}/docs/${docId}`, {
    method: "PUT",
    body: JSON.stringify({ title }),
  });
}

export async function deleteDoc(
  engagementId: string,
  docId: string,
): Promise<void> {
  await request<void>(`/engagements/${engagementId}/docs/${docId}`, {
    method: "DELETE",
  });
}

// ---- Doc viewer endpoints (Sprint 1) -------------------------------------

export interface ApiDocBlock {
  id: string;
  document_id: string;
  position: number;
  payload: unknown; // JSON — e.g. { type: "markdown", text: "..." }
  created_at: string;
}

export interface ApiDocWithCount extends ApiDoc {
  block_count: number;
}

export async function listEngagementDocs(
  token: string,
  engagementId: string,
): Promise<ApiDocWithCount[]> {
  const r = await fetch(
    `${API_BASE}/engagements/${engagementId}/docs`,
    { headers: { authorization: `Bearer ${token}` } },
  );
  if (!r.ok) throw new Error(`listEngagementDocs: ${r.status}`);
  return r.json();
}

export async function getDoc(token: string, id: string): Promise<ApiDoc> {
  const r = await fetch(`${API_BASE}/docs/${id}`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`getDoc: ${r.status}`);
  return r.json();
}

export async function listDocBlocks(
  token: string,
  docId: string,
): Promise<ApiDocBlock[]> {
  const r = await fetch(`${API_BASE}/docs/${docId}/blocks`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listDocBlocks: ${r.status}`);
  return r.json();
}

// ---- Chat endpoint -------------------------------------------------------

export async function sendChatMessage(
  message: string,
  context?: string,
  provider?: string,
  model?: string,
  apiKey?: string,
): Promise<string> {
  const res = await fetch(`${API_BASE}/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, context, provider, model, apiKey }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: "Chat request failed" }));
    throw new Error(err.error || "Chat request failed");
  }
  const data = await res.json();
  return data.reply;
}

// ---- Project endpoints (Sprint 0) -------------------------------------------

export interface ApiProject { id: string; engagement_id: string; name: string; created_at: string }

export async function listProjects(token: string): Promise<ApiProject[]> {
  const r = await fetch(`${API_BASE}/projects`, { headers: { authorization: `Bearer ${token}` } });
  if (!r.ok) throw new Error(`listProjects: ${r.status}`);
  return r.json();
}

export async function createProject(token: string, name: string): Promise<ApiProject> {
  const r = await fetch(`${API_BASE}/projects`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ name }),
  });
  if (!r.ok) throw new Error(`createProject: ${r.status}`);
  return r.json();
}

// ---- Project member endpoints (Sprint 1) ------------------------------------

export interface ApiProjectMember {
  id: string;
  project_id: string;
  name: string;
  description: string;
  graph_json: {
    nodes: { id: string; type: string; config: Record<string, unknown> }[];
    edges: { from: string; to: string }[];
  };
  provider_config_json: Record<string, unknown>;
  version: number;
}

export async function createMember(
  token: string,
  body: {
    project_id: string;
    name: string;
    graph_json: ApiProjectMember["graph_json"];
    provider_config_json?: Record<string, unknown>;
  },
): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ provider_config_json: {}, ...body }),
  });
  if (!r.ok) throw new Error(`createMember: ${r.status}`);
  return r.json();
}

export async function updateMember(
  token: string,
  id: string,
  body: {
    name: string;
    graph_json: ApiProjectMember["graph_json"];
    provider_config_json?: Record<string, unknown>;
  },
): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members/${id}`, {
    method: "PUT",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ provider_config_json: {}, ...body }),
  });
  if (!r.ok) throw new Error(`updateMember: ${r.status}`);
  return r.json();
}

export async function runMemberNow(
  token: string,
  id: string,
): Promise<{ run_id: string }> {
  const r = await fetch(`${API_BASE}/project_members/${id}/run`, {
    method: "POST",
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`runMemberNow: ${r.status}`);
  return r.json();
}

export async function listMembers(
  token: string,
  projectId: string,
): Promise<ApiProjectMember[]> {
  const r = await fetch(`${API_BASE}/projects/${projectId}/members`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listMembers: ${r.status}`);
  return r.json();
}

export async function getMember(
  token: string,
  id: string,
): Promise<ApiProjectMember> {
  const r = await fetch(`${API_BASE}/project_members/${id}`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`getMember: ${r.status}`);
  return r.json();
}

// ---- Fleet (Sprint 1 Task 12) ---------------------------------------------

export interface FleetRow {
  id: string;
  name: string;
  project_id: string;
  last_status: string | null;
  next_fire_at: string | null;
  running: boolean;
}

export async function fetchFleet(token: string): Promise<FleetRow[]> {
  const r = await fetch(`${API_BASE}/project_members/fleet`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`fetchFleet: ${r.status}`);
  return r.json();
}

// ---- Schedule endpoints (Sprint 2) ----------------------------------------

export interface ApiSchedule {
  id: string;
  project_member_id: string;
  cron_expr: string;
  timezone: string;
  next_fire_at: string;
  last_fired_at: string | null;
  enabled: boolean;
}

export interface CreateScheduleRequest {
  project_member_id: string;
  cron_expr: string;
  timezone: string;
  enabled?: boolean;
}

export async function createSchedule(
  token: string,
  body: CreateScheduleRequest,
): Promise<ApiSchedule> {
  const r = await fetch(`${API_BASE}/schedules`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify(body),
  });
  if (!r.ok) throw new Error(`createSchedule: ${r.status}`);
  return r.json();
}

export async function listSchedules(
  token: string,
  projectMemberId: string,
): Promise<ApiSchedule[]> {
  const r = await fetch(
    `${API_BASE}/schedules?project_member_id=${projectMemberId}`,
    { headers: { authorization: `Bearer ${token}` } },
  );
  if (!r.ok) throw new Error(`listSchedules: ${r.status}`);
  return r.json();
}

export async function updateSchedule(
  token: string,
  id: string,
  body: CreateScheduleRequest,
): Promise<ApiSchedule> {
  const r = await fetch(`${API_BASE}/schedules/${id}`, {
    method: "PUT",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify(body),
  });
  if (!r.ok) throw new Error(`updateSchedule: ${r.status}`);
  return r.json();
}

export async function deleteSchedule(token: string, id: string): Promise<void> {
  const r = await fetch(`${API_BASE}/schedules/${id}`, {
    method: "DELETE",
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`deleteSchedule: ${r.status}`);
}

// ---- Run history (Sprint 2) ----------------------------------------------

export interface ApiRunSummary {
  id: string;
  project_member_id: string | null;
  engagement_id: string;
  status: string;
  triggered_by: string;
  created_at: string;
}

export async function listRuns(
  token: string,
  projectMemberId: string,
  limit?: number,
): Promise<ApiRunSummary[]> {
  const params = new URLSearchParams({ project_member_id: projectMemberId });
  if (limit !== undefined) params.set("limit", String(limit));
  const r = await fetch(`${API_BASE}/runs?${params.toString()}`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listRuns: ${r.status}`);
  return r.json();
}

// ---- Workspace assistant + provider introspection -------------------------

export interface AssistantChatResponse {
  provider_id: string;
  model: string;
  raw: unknown;
}

export async function assistantChat(
  token: string,
  messages: Array<{ role: string; content: string }>,
  providerId?: string,
  model?: string,
): Promise<AssistantChatResponse> {
  const r = await fetch(`${API_BASE}/assistant/chat`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      authorization: `Bearer ${token}`,
    },
    body: JSON.stringify({
      messages,
      provider_id: providerId,
      model,
    }),
  });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`assistantChat: ${r.status}${body ? ` ${body}` : ""}`);
  }
  return r.json();
}

// ---- Streaming assistant chat (SSE over POST) ----------------------------
//
// `POST /api/assistant/chat/stream` emits Server-Sent Events:
//   event: delta\ndata: <chunk>\n\n   — per-token text
//   event: error\ndata: <message>\n\n — terminal
//   event: done\ndata: \n\n           — clean completion
//
// Browser `EventSource` doesn't support custom request headers (so it can't
// send a Bearer token) nor request bodies. We therefore drive the stream
// manually with `fetch` + a TextDecoder over `response.body`.

export interface ChatStreamHandlers {
  onDelta: (text: string) => void;
  onDone?: () => void;
  onError?: (msg: string) => void;
  signal?: AbortSignal;
}

export async function assistantChatStream(
  token: string,
  messages: Array<{ role: string; content: string }>,
  providerId: string | undefined,
  model: string | undefined,
  handlers: ChatStreamHandlers,
): Promise<void> {
  let r: Response;
  try {
    r = await fetch(`${API_BASE}/assistant/chat/stream`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        authorization: `Bearer ${token}`,
      },
      body: JSON.stringify({
        messages,
        provider_id: providerId,
        model,
      }),
      signal: handlers.signal,
    });
  } catch (err) {
    // Includes AbortError when the caller cancels before headers arrive.
    handlers.onError?.(err instanceof Error ? err.message : String(err));
    return;
  }
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    handlers.onError?.(`HTTP ${r.status}: ${body}`);
    return;
  }
  const reader = r.body?.getReader();
  if (!reader) {
    handlers.onError?.("no response body");
    return;
  }
  const decoder = new TextDecoder();
  let buf = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buf += decoder.decode(value, { stream: true });
      // SSE frames are separated by a blank line ("\n\n").
      let sepIdx: number;
      while ((sepIdx = buf.indexOf("\n\n")) >= 0) {
        const frame = buf.slice(0, sepIdx);
        buf = buf.slice(sepIdx + 2);
        let eventName = "delta";
        let data = "";
        for (const line of frame.split("\n")) {
          if (line.startsWith("event:")) {
            eventName = line.slice(6).trim();
          } else if (line.startsWith("data:")) {
            // SSE allows multi-line data: join with "\n", strip the one
            // leading space that "data: <text>" typically carries.
            data += (data ? "\n" : "") + line.slice(5).replace(/^ /, "");
          }
        }
        if (eventName === "delta") {
          handlers.onDelta(data);
        } else if (eventName === "error") {
          handlers.onError?.(data);
          return;
        } else if (eventName === "done") {
          handlers.onDone?.();
          return;
        }
      }
    }
  } catch (err) {
    if ((err as { name?: string })?.name === "AbortError") {
      // Caller-initiated cancel; treat as quiet termination.
      return;
    }
    handlers.onError?.(err instanceof Error ? err.message : String(err));
  }
}

export async function listProviders(token: string): Promise<string[]> {
  const r = await fetch(`${API_BASE}/providers`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listProviders: ${r.status}`);
  const body = await r.json();
  return body.providers ?? [];
}

// ---- Per-engagement provider keys ----------------------------------------
//
// Phase 1: storage + retrieval. The backend never returns plaintext keys —
// only a masked `key_preview` such as `"sk-a...xyz"`. PUT replaces an
// existing key (upsert).

export interface ApiProviderKey {
  provider_id: string;
  key_preview: string;
  created_at: string;
  updated_at: string;
}

export async function listProviderKeys(token: string): Promise<ApiProviderKey[]> {
  const r = await fetch(`${API_BASE}/providers/keys`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listProviderKeys: ${r.status}`);
  return r.json();
}

export async function setProviderKey(
  token: string,
  providerId: string,
  key: string,
): Promise<ApiProviderKey> {
  const r = await fetch(`${API_BASE}/providers/keys/${providerId}`, {
    method: "PUT",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ key }),
  });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`setProviderKey: ${r.status}${body ? ` ${body}` : ""}`);
  }
  return r.json();
}

export async function deleteProviderKey(
  token: string,
  providerId: string,
): Promise<void> {
  const r = await fetch(`${API_BASE}/providers/keys/${providerId}`, {
    method: "DELETE",
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok && r.status !== 404) {
    throw new Error(`deleteProviderKey: ${r.status}`);
  }
}

// ---- Workspace metrics summary -------------------------------------------
//
// `GET /api/metrics/summary` returns a point-in-time aggregation of
// engagement-scoped counts. Shape mirrors `MetricsSummary` on the backend.
// Used by the Settings page's "Workspace Stats" card grid.

export interface ApiMetricsSummary {
  engagements_total: number;
  projects_total: number;
  members_total: number;
  members_by_agent_type: Record<string, number>;
  schedules_total: number;
  schedules_enabled: number;
  runs_24h: number;
  runs_succeeded_24h: number;
  runs_failed_24h: number;
  docs_total: number;
  plugins_installed: number;
  providers_configured: string[];
}

export async function fetchMetricsSummary(
  token: string,
): Promise<ApiMetricsSummary> {
  const r = await fetch(`${API_BASE}/metrics/summary`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`fetchMetricsSummary: ${r.status}${body ? ` ${body}` : ""}`);
  }
  return r.json();
}

// ---- Plugin endpoints ----------------------------------------------------
//
// The backend stores a plugin "install" as a row in plugin_installs joined to
// the caller's engagement. The full manifest is persisted as JSON; capability
// arrays live inside `manifest.inbound` / `manifest.outbound`.

export interface PluginCapability {
  id: string;
  kind: string;
  input_schema?: unknown;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  inbound?: PluginCapability[];
  outbound?: PluginCapability[];
}

/** Row returned by `GET /api/plugins`. */
export interface ApiPluginInstall {
  id: string;            // install uuid
  engagement_id: string;
  plugin_id: string;     // manifest.id
  manifest: PluginManifest;
  installed_at: string;
}

export async function listInstalledPlugins(
  token: string,
): Promise<ApiPluginInstall[]> {
  const r = await fetch(`${API_BASE}/plugins`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listInstalledPlugins: ${r.status}`);
  return r.json();
}

export async function installPlugin(
  token: string,
  manifest: PluginManifest,
): Promise<{ id: string }> {
  const r = await fetch(`${API_BASE}/plugins`, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ manifest }),
  });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`installPlugin: ${r.status}${body ? ` ${body}` : ""}`);
  }
  return r.json();
}

export async function setPluginCredentials(
  token: string,
  installId: string,
  direction: "inbound" | "outbound",
  payload: Record<string, string>,
): Promise<void> {
  const r = await fetch(`${API_BASE}/plugins/${installId}/credentials`, {
    method: "PUT",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify({ direction, payload }),
  });
  if (!r.ok) {
    const body = await r.text().catch(() => "");
    throw new Error(`setPluginCredentials: ${r.status}${body ? ` ${body}` : ""}`);
  }
}

