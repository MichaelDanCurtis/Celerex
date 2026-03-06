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

export async function sendChatMessage(message: string, context?: string): Promise<string> {
  const res = await fetch(`${API_BASE}/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ message, context }),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({ error: "Chat request failed" }));
    throw new Error(err.error || "Chat request failed");
  }
  const data = await res.json();
  return data.reply;
}
