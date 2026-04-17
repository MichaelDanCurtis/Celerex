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

