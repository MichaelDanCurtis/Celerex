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

export async function listProviders(token: string): Promise<string[]> {
  const r = await fetch(`${API_BASE}/providers`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!r.ok) throw new Error(`listProviders: ${r.status}`);
  const body = await r.json();
  return body.providers ?? [];
}

