export type Locale = "zh" | "en"

export type Me = { user_id: string; is_root: boolean; setup_required: boolean }
export type Credential = { id: string; owner_id: string; exchange: string; label: string; created_at: number; updated_at: number }
export type Trader = { id: string; owner_id: string; name: string; template_id: string; credential_id: string; params: Record<string, unknown>; signal: Record<string, unknown>; signal_token_prefix: string; enabled: boolean; status: string; last_run_at?: number | null; last_signal_at?: number | null; last_error?: string | null; successful_runs: number; created_at: number; updated_at: number }
export type SignalHistory = { id: string; trader_id: string; signal: Record<string, unknown>; occurrences: number; created_at: number; updated_at: number }
export type JsonSchema = { type?: string; title?: string; description?: string; required?: string[]; properties?: Record<string, JsonSchema> }
export type Template = { id: string; name: string; credential_type: string; exchange: string; description: string; params_schema: JsonSchema; signal_schema: JsonSchema; params_example: Record<string, unknown>; signal_example: Record<string, unknown> }
export type LinkitSettings = { owner_id: string; recipient_username: string; configured: boolean; updated_at: number } | null
export type SystemResources = { sampled_at: number; cpu: { usage_percent: number; load_1m: number; logical_cpus: number }; memory: { used_bytes: number; total_bytes: number; available_bytes: number }; disk: { mount_point: string; used_bytes: number; total_bytes: number; available_bytes: number } | null; sqlite: { main_bytes: number; wal_bytes: number; shm_bytes: number; total_bytes: number } }
