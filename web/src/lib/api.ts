export function request<T>(path: string, accessToken: string, init?: RequestInit): Promise<T> {
  return fetch(path, { ...init, headers: { "Content-Type": "application/json", Authorization: `Bearer ${accessToken}`, ...init?.headers } }).then(async response => {
    if (response.status === 204) return undefined as T
    const body = await response.json() as T & { error?: string }
    if (!response.ok) throw new Error(body.error ?? "Request failed")
    return body
  })
}
