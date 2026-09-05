import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { ActivityIcon, CircleAlertIcon, KeyRoundIcon } from "lucide-react"
import { useNavigate, useParams } from "react-router-dom"
import { toast } from "sonner"

import { request } from "../lib/api"
import { formatTime, showError } from "../lib/format"
import { localeText, type Copy } from "../lib/i18n"
import type { Run, Trader } from "../lib/types"
import { EmptyState, PageError, StatusBadge, TraderEnabledSwitch } from "../components/trader-ui"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"

export function TraderDetailPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const { traderId } = useParams()
  const trader = useQuery({ queryKey: ["trader", traderId, token], queryFn: () => request<Trader>(`/api/v1/traders/${traderId}`, token), enabled: Boolean(traderId) })
  const runs = useQuery({ queryKey: ["runs", traderId, token], queryFn: () => request<Run[]>(`/api/v1/traders/${traderId}/runs`, token), enabled: Boolean(traderId), refetchInterval: 10_000 })
  if (!traderId) return null
  if (trader.isPending) return <Skeleton className="h-72" />
  if (trader.error) return <PageError error={trader.error} />
  if (!trader.data) return null

  const value = trader.data
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><Button variant="ghost" size="sm" onClick={() => navigate("/traders")}>← {t.traders}</Button><h1 className="mt-2 mb-0 text-2xl font-semibold tracking-tight">{value.name}</h1><p className="mt-1 font-mono text-xs text-muted-foreground">{value.id}</p></div><div className="flex items-center gap-3"><StatusBadge trader={value} t={t} /><TraderEnabledSwitch trader={value} token={token} t={t} onChanged={onChanged} /></div></div>{value.last_error && <Alert variant="destructive"><CircleAlertIcon /><AlertTitle>{t.failed}</AlertTitle><AlertDescription>{value.last_error}</AlertDescription></Alert>}<div className="grid gap-4 lg:grid-cols-2"><Card><CardHeader><CardTitle>{t.params}</CardTitle><CardDescription>{t.credential}: {value.credential_id}</CardDescription></CardHeader><CardContent><JsonBlock value={value.params} /></CardContent></Card><SignalCard key={JSON.stringify(value.signal)} trader={value} token={token} t={t} onSaved={() => { void trader.refetch(); onChanged() }} /></div><Card><CardHeader><CardTitle>{t.runs}</CardTitle><CardDescription>{t.detail}</CardDescription></CardHeader><CardContent>{runs.data?.length ? <Table><TableHeader><TableRow><TableHead>{t.status}</TableHead><TableHead>{t.updated}</TableHead><TableHead>{localeText(t, "Summary", "摘要")}</TableHead></TableRow></TableHeader><TableBody>{runs.data.map(run => <TableRow key={run.id}><TableCell><Badge variant={run.status === "failed" ? "destructive" : "default"}>{run.status}</Badge></TableCell><TableCell>{formatTime(run.started_at)}</TableCell><TableCell className="max-w-xl truncate font-mono text-xs">{run.summary}</TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.runs} description={localeText(t, "No execution has been recorded yet.", "尚未记录执行。")} icon={ActivityIcon} />}</CardContent></Card></div>
}

function SignalCard({ trader, token, t, onSaved }: { trader: Trader; token: string; t: Copy; onSaved: () => void }) {
  const [payload, setPayload] = useState(() => JSON.stringify(trader.signal, null, 2))
  const [error, setError] = useState<string | null>(null)
  const save = useMutation({
    mutationFn: (serialized: string) => {
      let signal: unknown
      try {
        signal = JSON.parse(serialized)
      } catch {
        throw new Error(t.signalJsonInvalid)
      }
      if (!signal || typeof signal !== "object" || Array.isArray(signal)) throw new Error(t.signalJsonObject)
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

  return <Card><CardHeader><CardTitle>{t.manualSignal}</CardTitle><CardDescription>{t.manualSignalDescription}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">{trader.enabled && <Alert><CircleAlertIcon /><AlertTitle>{t.manualSignalRiskTitle}</AlertTitle><AlertDescription>{t.manualSignalRisk}</AlertDescription></Alert>}<form className="flex flex-col gap-4" onSubmit={submit}><FieldGroup><Field data-invalid={Boolean(error)}><FieldLabel htmlFor={`signal-payload-${trader.id}`}>{t.signalPayload}</FieldLabel><Textarea id={`signal-payload-${trader.id}`} value={payload} onChange={event => { setPayload(event.target.value); setError(null) }} aria-invalid={Boolean(error)} className="min-h-56 font-mono text-xs leading-5" spellCheck={false} disabled={save.isPending} /><FieldDescription>{t.signal}: {trader.signal_token_prefix}…</FieldDescription><FieldError>{error}</FieldError></Field></FieldGroup><div className="flex justify-end"><Button type="submit" disabled={save.isPending}>{save.isPending ? t.saving : t.save}</Button></div></form><Button variant="outline" onClick={() => rotate.mutate()} disabled={rotate.isPending}><KeyRoundIcon data-icon="inline-start" />{rotate.isPending ? t.rotating : t.token}</Button><p className="text-xs text-muted-foreground">{t.usage}</p></CardContent></Card>
}

function JsonBlock({ value }: { value: Record<string, unknown> }) {
  return <pre className="m-0 max-h-72 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap break-words">{JSON.stringify(value, null, 2)}</pre>
}
