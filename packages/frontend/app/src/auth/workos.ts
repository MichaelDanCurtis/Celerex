// WorkOS hosted login for the OmniVyx SPA — the ONE identity login for the whole
// service. The browser only performs the redirect; the Agent (the one server,
// holding the confidential client secret) exchanges the returned authorization
// code for the engagement session. The client id is non-secret; override via
// VITE_WORKOS_CLIENT_ID.
const CLIENT_ID =
  (import.meta.env.VITE_WORKOS_CLIENT_ID as string | undefined) ||
  "client_01KV9VXPN6DK8PG87WW6C80190";

const WORKOS_API = "https://api.workos.com";

/** Hosted AuthKit authorization URL; AuthKit redirects back to this origin with ?code=. */
export function workosAuthorizeUrl(): string {
  const params = new URLSearchParams({
    response_type: "code",
    client_id: CLIENT_ID,
    redirect_uri: window.location.origin,
    provider: "authkit",
  });
  return `${WORKOS_API}/user_management/authorize?${params.toString()}`;
}

/** Send the browser to the WorkOS hosted login. */
export function workosSignIn(): void {
  window.location.assign(workosAuthorizeUrl());
}

/**
 * If this load is the AuthKit return leg, read the `?code=` and strip it (and the
 * `state`) from the URL so a refresh doesn't replay it. Returns the code or null.
 */
export function consumeWorkosCode(): string | null {
  const url = new URL(window.location.href);
  const code = url.searchParams.get("code");
  if (!code) return null;
  url.searchParams.delete("code");
  url.searchParams.delete("state");
  window.history.replaceState({}, "", url.pathname + url.search + url.hash);
  return code;
}
