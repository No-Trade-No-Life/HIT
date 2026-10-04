import type { AuthMiniContextValue } from "auth-mini-react-components"

export type AuthSdk = NonNullable<AuthMiniContextValue["sdk"]>

// RECOVERY: the Browser SDK rotates access tokens without re-rendering
// consumers, so tokens are resolved when a request is sent. A stale snapshot
// gets a single retry after a session refresh; a failed refresh falls back to
// the original 401 response so callers surface the real authentication error.
export async function request<T>(path: string, sdk?: AuthSdk, init?: RequestInit): Promise<T> {
  const send = (accessToken?: string) =>
    fetch(path, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(accessToken ? { Authorization: `Bearer ${accessToken}` } : {}),
        ...init?.headers,
      },
    })
  let response = await send(sdk?.session.getState().accessToken ?? undefined)
  if (response.status === 401 && sdk) {
    const refreshed = await sdk.session.refresh().catch(() => null)
    if (refreshed?.accessToken) response = await send(refreshed.accessToken)
  }
  if (response.status === 204) return undefined as T
  const body = await response.json() as T & { error?: string }
  if (!response.ok) throw new Error(body.error ?? "Request failed")
  return body
}
