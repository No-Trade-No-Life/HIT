export type Locale = "zh" | "en"

export type Me = { user_id: string; is_root: boolean; setup_required: boolean }
export type Credential = { id: string; owner_id: string; exchange: string; label: string; created_at: number; updated_at: number }
export type Trader = { id: string; owner_id: string; name: string; template_id: string; credential_id: string; params: Record<string, unknown>; signal: Record<string, unknown>; signal_token_prefix: string; enabled: boolean; status: string; last_run_at?: number; last_error?: string; updated_at: number }
export type Run = { id: string; trader_id: string; status: string; summary?: string; started_at: number; finished_at: number }
export type Template = { id: string; exchange: string; title: string; description: string; params_example: Record<string, unknown>; signal_example: Record<string, unknown> }
export type LinkitSettings = { owner_id: string; recipient_username: string; configured: boolean; updated_at: number } | null
