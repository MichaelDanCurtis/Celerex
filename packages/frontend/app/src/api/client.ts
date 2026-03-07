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

// ---- Copilot auth endpoints -----------------------------------------------

export async function startCopilotAuth(): Promise<{ user_code: string; verification_uri: string; device_code: string }> {
  return request("/copilot/auth/start", { method: "POST" });
}

export async function pollCopilotAuth(deviceCode: string): Promise<{ status: "pending" | "complete" }> {
  return request("/copilot/auth/poll", { method: "POST", body: JSON.stringify({ device_code: deviceCode }) });
}

export async function getCopilotAuthStatus(): Promise<{ authenticated: boolean }> {
  return request("/copilot/auth/status");
}

export interface CopilotModel {
  id: string;
  name: string;
  vendor: string;
}

export async function fetchCopilotModels(): Promise<CopilotModel[]> {
  const data = await request<{ data: Array<{ id: string; name: string; vendor: string; capabilities?: { type?: string } }> }>("/copilot/models");
  // Filter to chat models only, deduplicate by id
  const seen = new Set<string>();
  return data.data
    .filter((m) => m.capabilities?.type === "chat")
    .filter((m) => {
      if (seen.has(m.id)) return false;
      seen.add(m.id);
      return true;
    })
    .map((m) => ({ id: m.id, name: m.name, vendor: m.vendor }));
}
