import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { CircleAlertIcon, HistoryIcon, KeyRoundIcon } from "lucide-react"
import { useNavigate, useParams } from "react-router-dom"
import { toast } from "sonner"

import { request } from "../lib/api"
import { formatTime, showError } from "../lib/format"
import type { Copy } from "../lib/i18n"
import type { JsonSchema, SignalHistory, Template, Trader } from "../lib/types"
import { EmptyState, PageError, StatusBadge, TraderEnabledSwitch } from "../components/trader-ui"
import { SchemaValueList } from "../components/schema-value-list"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"

export function TraderDetailPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const { traderId } = useParams()
  const trader = useQuery({ queryKey: ["trader", traderId, token], queryFn: () => request<Trader>(`/api/v1/traders/${traderId}`, token), enabled: Boolean(traderId) })
  const signalHistory = useQuery({ queryKey: ["signal-history", traderId, token], queryFn: () => request<SignalHistory[]>(`/api/v1/traders/${traderId}/signal-history`, token), enabled: Boolean(traderId), refetchInterval: 10_000 })
  const templates = useQuery({ queryKey: ["templates", token], queryFn: () => request<Template[]>("/api/templates", token) })
  if (!traderId) return null
  if (trader.isPending) return <Skeleton className="h-72" />
  if (trader.error) return <PageError error={trader.error} />
  if (!trader.data) return null

  const value = trader.data
  const template = templates.data?.find(item => item.id === value.template_id)
  const refresh = () => { void trader.refetch(); void signalHistory.refetch(); onChanged() }
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><Button variant="ghost" size="sm" onClick={() => navigate("/traders")}>← {t.traders}</Button><h1 className="mt-2 mb-0 text-2xl font-semibold tracking-tight">{value.name}</h1><p className="mt-1 font-mono text-xs text-muted-foreground">{value.id}</p></div><div className="flex items-center gap-3"><StatusBadge trader={value} t={t} /><TraderEnabledSwitch trader={value} token={token} t={t} onChanged={onChanged} /></div></div>{value.last_error && <Alert variant="destructive"><CircleAlertIcon /><AlertTitle>{t.failed}</AlertTitle><AlertDescription>{value.last_error}</AlertDescription></Alert>}<div className="grid gap-4 lg:grid-cols-2"><ParamsCard key={JSON.stringify(value.params)} trader={value} schema={template?.params_schema} token={token} t={t} onSaved={refresh} /><SignalCard key={JSON.stringify(value.signal)} trader={value} schema={template?.signal_schema} token={token} t={t} onSaved={refresh} /></div><Card><CardHeader><CardTitle>{t.signalHistory}</CardTitle><CardDescription>{t.signalHistoryDescription}</CardDescription></CardHeader><CardContent>{signalHistory.data?.length ? <Table><TableHeader><TableRow><TableHead>{t.signalPayload}</TableHead><TableHead>{t.occurrences}</TableHead><TableHead>{t.firstSeen}</TableHead><TableHead>{t.updated}</TableHead></TableRow></TableHeader><TableBody>{signalHistory.data.map(entry => <TableRow key={entry.id}><TableCell className="min-w-64 align-top"><JsonBlock value={entry.signal} /></TableCell><TableCell><Badge variant="secondary">{entry.occurrences}</Badge></TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.created_at)}</TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.updated_at)}</TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.signalHistory} description={t.signalHistoryDescription} icon={HistoryIcon} />}</CardContent></Card></div>
}

function ParamsCard({ trader, schema, token, t, onSaved }: { trader: Trader; schema?: JsonSchema; token: string; t: Copy; onSaved: () => void }) {
  const [payload, setPayload] = useState(() => JSON.stringify(trader.params, null, 2))
  const [error, setError] = useState<string | null>(null)
  const save = useMutation({
    mutationFn: (serialized: string) => {
      const params = parseObject(serialized, t.paramsJsonInvalid, t.paramsJsonObject)
      return request<Trader>(`/api/v1/traders/${trader.id}/params`, token, { method: "PATCH", body: JSON.stringify({ params }) })
    },
    onSuccess: () => { toast.success(t.paramsSaved); onSaved() },
    onError: (requestError: Error) => { setError(requestError.message); showError(requestError) },
  })
  const submit = (event: React.FormEvent<HTMLFormElement>) => { event.preventDefault(); setError(null); save.mutate(payload) }

  return <Card><CardHeader><CardTitle>{t.manualParams}</CardTitle><CardDescription>{t.manualParamsDescription}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">{trader.enabled && <Alert><CircleAlertIcon /><AlertTitle>{t.manualParamsRiskTitle}</AlertTitle><AlertDescription>{t.manualParamsRisk}</AlertDescription></Alert>}<div className="flex items-center justify-between border-b pb-3 text-sm"><span className="text-muted-foreground">{t.successfulRuns}</span><span className="font-mono tabular-nums">{trader.successful_runs}</span></div><Tabs defaultValue="structured"><TabsList><TabsTrigger value="structured">{t.structured}</TabsTrigger><TabsTrigger value="json">JSON</TabsTrigger></TabsList><TabsContent value="structured"><SchemaValueList schema={schema} value={trader.params} /></TabsContent><JsonEditorTab id={`params-payload-${trader.id}`} label={t.paramsPayload} payload={payload} error={error} pending={save.isPending} onChange={value => { setPayload(value); setError(null) }} onSubmit={submit} t={t} /></Tabs></CardContent></Card>
}

function SignalCard({ trader, schema, token, t, onSaved }: { trader: Trader; schema?: JsonSchema; token: string; t: Copy; onSaved: () => void }) {
  const [payload, setPayload] = useState(() => JSON.stringify(trader.signal, null, 2))
  const [error, setError] = useState<string | null>(null)
  const save = useMutation({
    mutationFn: (serialized: string) => {
      const signal = parseObject(serialized, t.signalJsonInvalid, t.signalJsonObject)
      return request<Trader>(`/api/v1/traders/${trader.id}/signal`, token, { method: "PATCH", body: JSON.stringify({ signal }) })
    },
    onSuccess: () => { toast.success(t.signalSaved); onSaved() },
    onError: (requestError: Error) => { setError(requestError.message); showError(requestError) },
  })
  const rotate = useMutation({
    mutationFn: () => request<{ signal_token: string }>(`/api/v1/traders/${trader.id}/signal-token`, token, { method: "POST" }),
    onSuccess: response => { navigator.clipboard.writeText(response.signal_token).catch(() => undefined); toast.success(t.tokenRotated) },
    onError: showError,
  })
  const submit = (event: React.FormEvent<HTMLFormElement>) => { event.preventDefault(); setError(null); save.mutate(payload) }

  return <Card><CardHeader><CardTitle>{t.manualSignal}</CardTitle><CardDescription>{t.manualSignalDescription}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">{trader.enabled && <Alert><CircleAlertIcon /><AlertTitle>{t.manualSignalRiskTitle}</AlertTitle><AlertDescription>{t.manualSignalRisk}</AlertDescription></Alert>}<Tabs defaultValue="structured"><TabsList><TabsTrigger value="structured">{t.structured}</TabsTrigger><TabsTrigger value="json">JSON</TabsTrigger></TabsList><TabsContent value="structured"><SchemaValueList schema={schema} value={trader.signal} /></TabsContent><JsonEditorTab id={`signal-payload-${trader.id}`} label={t.signalPayload} description={`${t.signal}: ${trader.signal_token_prefix}…`} payload={payload} error={error} pending={save.isPending} onChange={value => { setPayload(value); setError(null) }} onSubmit={submit} t={t} /></Tabs><Button variant="outline" onClick={() => rotate.mutate()} disabled={rotate.isPending}><KeyRoundIcon data-icon="inline-start" />{rotate.isPending ? t.rotating : t.token}</Button><p className="text-xs text-muted-foreground">{t.usage}</p></CardContent></Card>
}

function JsonEditorTab({ id, label, description, payload, error, pending, onChange, onSubmit, t }: { id: string; label: string; description?: string; payload: string; error: string | null; pending: boolean; onChange: (value: string) => void; onSubmit: (event: React.FormEvent<HTMLFormElement>) => void; t: Copy }) {
  return <TabsContent value="json"><form className="flex flex-col gap-4" onSubmit={onSubmit}><FieldGroup><Field data-invalid={Boolean(error)}><FieldLabel htmlFor={id}>{label}</FieldLabel><Textarea id={id} value={payload} onChange={event => onChange(event.target.value)} aria-invalid={Boolean(error)} className="min-h-56 font-mono text-xs leading-5" spellCheck={false} disabled={pending} />{description && <FieldDescription>{description}</FieldDescription>}<FieldError>{error}</FieldError></Field></FieldGroup><div className="flex justify-end"><Button type="submit" disabled={pending}>{pending ? t.saving : t.save}</Button></div></form></TabsContent>
}

function parseObject(serialized: string, invalidJsonMessage: string, nonObjectMessage: string): Record<string, unknown> {
  let value: unknown
  try {
    value = JSON.parse(serialized)
  } catch {
    throw new Error(invalidJsonMessage)
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(nonObjectMessage)
  return value as Record<string, unknown>
}

function JsonBlock({ value }: { value: Record<string, unknown> }) {
  return <pre className="m-0 max-h-72 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap break-words">{JSON.stringify(value, null, 2)}</pre>
}
