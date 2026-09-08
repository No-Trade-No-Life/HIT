import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { CircleAlertIcon, CopyIcon, HistoryIcon, KeyRoundIcon, PencilIcon } from "lucide-react"
import { useNavigate, useParams } from "react-router-dom"
import { toast } from "sonner"

import { request } from "../lib/api"
import { formatTime, showError } from "../lib/format"
import { localeText, type Copy } from "../lib/i18n"
import type { JsonSchema, SignalHistory, Template, Trader } from "../lib/types"
import { EmptyState, PageError, StatusBadge, TraderEnabledSwitch } from "../components/trader-ui"
import { SchemaValueList } from "../components/schema-value-list"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldError, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
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
  return <div className="flex flex-col gap-6">
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div>
        <Button variant="ghost" size="sm" onClick={() => navigate("/traders")}>← {t.traders}</Button>
        <TraderNameEditor trader={value} token={token} t={t} onSaved={refresh} />
        <p className="mt-1 font-mono text-xs text-muted-foreground">{value.id}</p>
      </div>
      <div className="flex items-center gap-3"><StatusBadge trader={value} t={t} /><TraderEnabledSwitch trader={value} token={token} t={t} onChanged={onChanged} /></div>
    </div>
    {value.last_error && <Alert variant="destructive"><CircleAlertIcon /><AlertTitle>{t.failed}</AlertTitle><AlertDescription>{value.last_error}</AlertDescription></Alert>}
    <div className="grid gap-4 lg:grid-cols-2"><ParamsCard key={JSON.stringify(value.params)} trader={value} schema={template?.params_schema} token={token} t={t} onSaved={refresh} /><SignalCard key={JSON.stringify(value.signal)} trader={value} schema={template?.signal_schema} token={token} t={t} onSaved={refresh} /></div>
    <SignalIntegrationCard trader={value} template={template} token={token} t={t} />
    <Card><CardHeader><CardTitle>{t.signalHistory}</CardTitle><CardDescription>{t.signalHistoryDescription}</CardDescription></CardHeader><CardContent>{signalHistory.data?.length ? <Table><TableHeader><TableRow><TableHead>{t.signalPayload}</TableHead><TableHead>{t.occurrences}</TableHead><TableHead>{t.firstSeen}</TableHead><TableHead>{t.updated}</TableHead></TableRow></TableHeader><TableBody>{signalHistory.data.map(entry => <TableRow key={entry.id}><TableCell className="min-w-64 align-top"><JsonBlock value={entry.signal} /></TableCell><TableCell><Badge variant="secondary">{entry.occurrences}</Badge></TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.created_at)}</TableCell><TableCell className="whitespace-nowrap text-muted-foreground">{formatTime(entry.updated_at)}</TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.signalHistory} description={t.signalHistoryDescription} icon={HistoryIcon} />}</CardContent></Card>
  </div>
}

function TraderNameEditor({ trader, token, t, onSaved }: { trader: Trader; token: string; t: Copy; onSaved: () => void }) {
  const [editing, setEditing] = useState(false)
  const [name, setName] = useState(trader.name)
  const [error, setError] = useState<string | null>(null)
  const save = useMutation({
    mutationFn: () => request<Trader>(`/api/v1/traders/${trader.id}/name`, token, { method: "PATCH", body: JSON.stringify({ name }) }),
    onSuccess: () => { toast.success(t.traderNameSaved); setEditing(false); onSaved() },
    onError: (requestError: Error) => { setError(requestError.message); showError(requestError) },
  })
  if (!editing) return <div className="mt-2 flex flex-wrap items-center gap-2"><h1 className="m-0 text-2xl font-semibold tracking-tight">{trader.name}</h1><Button variant="ghost" size="sm" onClick={() => { setName(trader.name); setError(null); setEditing(true) }}><PencilIcon data-icon="inline-start" />{t.editTraderName}</Button></div>
  return <form className="mt-2" onSubmit={event => { event.preventDefault(); setError(null); save.mutate() }}><FieldGroup><Field data-invalid={Boolean(error)}><FieldLabel htmlFor={`trader-name-${trader.id}`}>{t.traderName}</FieldLabel><div className="flex flex-wrap items-center gap-2"><Input id={`trader-name-${trader.id}`} value={name} onChange={event => setName(event.target.value)} disabled={save.isPending} required /><Button type="submit" disabled={save.isPending}>{save.isPending ? t.saving : t.save}</Button><Button type="button" variant="outline" onClick={() => { setName(trader.name); setError(null); setEditing(false) }} disabled={save.isPending}>{t.cancel}</Button></div><FieldError>{error}</FieldError></Field></FieldGroup></form>
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
  const submit = (event: React.FormEvent<HTMLFormElement>) => { event.preventDefault(); setError(null); save.mutate(payload) }

  return <Card><CardHeader><CardTitle>{t.manualSignal}</CardTitle><CardDescription>{t.manualSignalDescription}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4">{trader.enabled && <Alert><CircleAlertIcon /><AlertTitle>{t.manualSignalRiskTitle}</AlertTitle><AlertDescription>{t.manualSignalRisk}</AlertDescription></Alert>}<Tabs defaultValue="structured"><TabsList><TabsTrigger value="structured">{t.structured}</TabsTrigger><TabsTrigger value="json">JSON</TabsTrigger></TabsList><TabsContent value="structured"><SchemaValueList schema={schema} value={trader.signal} /></TabsContent><JsonEditorTab id={`signal-payload-${trader.id}`} label={t.signalPayload} description={`${t.signal}: ${trader.signal_token_prefix}…`} payload={payload} error={error} pending={save.isPending} onChange={value => { setPayload(value); setError(null) }} onSubmit={submit} t={t} /></Tabs><p className="text-xs text-muted-foreground">{t.usage}</p></CardContent></Card>
}

function SignalIntegrationCard({ trader, template, token, t }: { trader: Trader; template?: Template; token: string; t: Copy }) {
  const copy = signalIntegrationCopy(t)
  const [signalToken, setSignalToken] = useState<string | null>(null)
  const endpoint = `${window.location.origin}/signal/v1/traders/${trader.id}`
  const schema = template?.signal_schema
  const payload = { signal: template?.signal_example ?? trader.signal }
  const visibleKey = signalToken ?? `${trader.signal_token_prefix}…`
  const curl = buildSignalCurl(endpoint, visibleKey, payload)
  const aiBrief = signalToken ? buildSignalIntegrationBrief(copy, trader, endpoint, signalToken, payload, schema) : null
  const rotate = useMutation({
    mutationFn: () => request<{ signal_token: string }>(`/api/v1/traders/${trader.id}/signal-token`, token, { method: "POST" }),
    onSuccess: response => {
      setSignalToken(response.signal_token)
      void copyText(buildSignalIntegrationBrief(copy, trader, endpoint, response.signal_token, payload, schema), copy.aiCopied, copy.copyFailed)
    },
    onError: showError,
  })
  if (!template) return <Card><CardHeader><CardTitle>{copy.title}</CardTitle><CardDescription>{copy.description}</CardDescription></CardHeader><CardContent><Skeleton className="h-72" /></CardContent></Card>

  return <Card>
    <CardHeader><CardTitle>{copy.title}</CardTitle><CardDescription>{copy.description}</CardDescription></CardHeader>
    <CardContent className="flex flex-col gap-5">
      <dl className="overflow-hidden rounded-md border text-sm">
        <IntegrationValue label={copy.traderId} value={trader.id} />
        <IntegrationValue label={copy.endpoint} value={endpoint} />
        <IntegrationValue label={copy.traderKey} value={visibleKey} secret={Boolean(signalToken)} />
      </dl>
      {signalToken ? <Alert><KeyRoundIcon /><AlertTitle>{copy.keyReady}</AlertTitle><AlertDescription className="flex flex-col gap-3"><code className="select-all break-all rounded-md bg-muted p-2 font-mono text-xs text-foreground">{signalToken}</code><div><Button type="button" variant="outline" size="sm" onClick={() => void copyText(signalToken, copy.keyCopied, copy.copyFailed)}><CopyIcon data-icon="inline-start" />{copy.copyTraderKey}</Button></div></AlertDescription></Alert> : <Alert><KeyRoundIcon /><AlertTitle>{copy.keyUnavailableTitle}</AlertTitle><AlertDescription>{copy.keyUnavailableDescription}</AlertDescription></Alert>}
      <div className="flex flex-wrap gap-2">
        {aiBrief ? <Button type="button" onClick={() => void copyText(aiBrief, copy.aiCopied, copy.copyFailed)}><CopyIcon data-icon="inline-start" />{copy.copyAiBrief}</Button> : <Button type="button" onClick={() => rotate.mutate()} disabled={rotate.isPending}><KeyRoundIcon data-icon="inline-start" />{rotate.isPending ? copy.rotating : copy.rotateAndCopyAiBrief}</Button>}
        {signalToken && <Button type="button" variant="outline" onClick={() => rotate.mutate()} disabled={rotate.isPending}><KeyRoundIcon data-icon="inline-start" />{rotate.isPending ? copy.rotating : copy.rotateKey}</Button>}
        <Button type="button" variant="outline" onClick={() => void copyText(curl, copy.curlCopied, copy.copyFailed)} disabled={!signalToken}><CopyIcon data-icon="inline-start" />{copy.copyCurl}</Button>
        <Button type="button" variant="outline" onClick={() => void copyText(JSON.stringify(payload, null, 2), copy.payloadCopied, copy.copyFailed)}><CopyIcon data-icon="inline-start" />{copy.copyPayload}</Button>
        <Button type="button" variant="outline" onClick={() => void copyText(JSON.stringify(schema ?? {}, null, 2), copy.schemaCopied, copy.copyFailed)}><CopyIcon data-icon="inline-start" />{copy.copySchema}</Button>
      </div>
      <Tabs defaultValue="curl">
        <TabsList><TabsTrigger value="curl">curl</TabsTrigger><TabsTrigger value="payload">{copy.payload}</TabsTrigger><TabsTrigger value="schema">JSON Schema</TabsTrigger></TabsList>
        <TabsContent value="curl"><CodeBlock value={curl} /></TabsContent>
        <TabsContent value="payload"><CodeBlock value={JSON.stringify(payload, null, 2)} /></TabsContent>
        <TabsContent value="schema"><CodeBlock value={JSON.stringify(schema ?? {}, null, 2)} /></TabsContent>
      </Tabs>
      <SignalSchemaGuide schema={schema} copy={copy} />
    </CardContent>
  </Card>
}

function IntegrationValue({ label, value, secret = false }: { label: string; value: string; secret?: boolean }) {
  return <div className="grid gap-2 border-b p-3 last:border-b-0 sm:grid-cols-[11rem_minmax(0,1fr)]"><dt className="font-medium text-muted-foreground">{label}</dt><dd className="m-0 min-w-0"><code className={secret ? "select-all break-all font-mono text-xs" : "break-all font-mono text-xs"}>{value}</code></dd></div>
}

function SignalSchemaGuide({ schema, copy }: { schema?: JsonSchema; copy: SignalIntegrationCopy }) {
  const required = new Set(schema?.required ?? [])
  const fields = Object.entries(schema?.properties ?? {})
  if (!fields.length) return <Skeleton className="h-28" />

  return <section className="flex flex-col gap-3">
    <div><h2 className="m-0 text-base font-medium">{copy.fieldContract}</h2><p className="mt-1 text-sm text-muted-foreground">{schema?.description}</p></div>
    <dl className="overflow-hidden rounded-md border">{fields.map(([name, field]) => <div className="grid gap-3 border-b p-3 last:border-b-0 sm:grid-cols-[minmax(14rem,0.7fr)_minmax(0,1fr)] sm:gap-6" key={name}>
      <dt className="flex flex-col gap-1"><div className="flex flex-wrap items-center gap-2"><code className="font-mono text-xs">{name}</code><Badge variant={required.has(name) ? "default" : "secondary"}>{required.has(name) ? copy.required : copy.optional}</Badge></div><span className="font-medium">{field.title ?? name}</span></dt>
      <dd className="m-0 flex flex-col gap-1 text-sm"><span>{field.description}</span><code className="text-xs text-muted-foreground">{field.type ?? "unknown"}</code></dd>
    </div>)}</dl>
  </section>
}

type SignalIntegrationCopy = {
  title: string
  description: string
  traderId: string
  endpoint: string
  traderKey: string
  keyReady: string
  keyUnavailableTitle: string
  keyUnavailableDescription: string
  rotateAndCopyAiBrief: string
  rotateKey: string
  rotating: string
  copyAiBrief: string
  copyTraderKey: string
  copyCurl: string
  copyPayload: string
  copySchema: string
  aiCopied: string
  keyCopied: string
  curlCopied: string
  payloadCopied: string
  schemaCopied: string
  copyFailed: string
  payload: string
  fieldContract: string
  required: string
  optional: string
  aiPromptIntro: string
  aiPromptEndpoint: string
  aiPromptTraderId: string
  aiPromptTraderKey: string
  aiPromptRules: string
  aiPromptRuleLines: string[]
  aiPromptSchema: string
}

function signalIntegrationCopy(t: Copy): SignalIntegrationCopy {
  const text = (english: string, chinese: string) => localeText(t, english, chinese)
  const chinese = t.overview === "总览"
  return {
    title: text("Signal integration", "信号接入"),
    description: text("Use this trader's dedicated PATCH endpoint to update only its target signal.", "使用此交易者专属的 PATCH 端点，只更新目标信号。"),
    traderId: "Trader ID",
    endpoint: text("Signal endpoint", "信号端点"),
    traderKey: "Trader Key",
    keyReady: text("New Trader Key — shown once", "新的 Trader Key（仅本次显示）"),
    keyUnavailableTitle: text("A full Trader Key is required", "需要完整的 Trader Key"),
    keyUnavailableDescription: text("HIT stores only the key hash, so the existing key cannot be shown again. Generate a new key below; it immediately invalidates the old integration and copies an AI-ready brief.", "HIT 只保存密钥哈希，无法再次显示旧密钥。请在下方生成新密钥；旧接入会立即失效，并会复制一份可直接交给 AI 的接入说明。"),
    rotateAndCopyAiBrief: text("Rotate key and copy AI brief", "轮换密钥并复制 AI 接入说明"),
    rotateKey: text("Rotate Trader Key", "轮换 Trader Key"),
    rotating: text("Rotating…", "轮换中…"),
    copyAiBrief: text("Copy AI integration brief", "复制 AI 接入说明"),
    copyTraderKey: text("Copy Trader Key", "复制 Trader Key"),
    copyCurl: text("Copy curl", "复制 curl"),
    copyPayload: text("Copy payload", "复制 payload"),
    copySchema: text("Copy JSON Schema", "复制 JSON Schema"),
    aiCopied: text("AI integration brief copied", "AI 接入说明已复制"),
    keyCopied: text("Trader Key copied", "Trader Key 已复制"),
    curlCopied: text("curl copied", "curl 已复制"),
    payloadCopied: text("Payload copied", "payload 已复制"),
    schemaCopied: text("JSON Schema copied", "JSON Schema 已复制"),
    copyFailed: text("Copy failed. Select the displayed text and copy it manually.", "复制失败，请选中展示的内容手动复制。"),
    payload: text("Payload example", "Payload 示例"),
    fieldContract: text("Signal field contract", "信号字段契约"),
    required: text("Required", "必填"),
    optional: text("Optional", "可选"),
    aiPromptIntro: text("Integrate an external signal source with the following HIT trader. Implement only this signal PATCH call; do not call management APIs or modify execution parameters, credentials, or the run switch.", "请为下列 HIT 交易者接入外部信号。只实现这个信号 PATCH 调用；不要调用管理 API，也不要修改执行参数、交易凭证或运行开关。"),
    aiPromptEndpoint: text("Signal endpoint", "信号端点"),
    aiPromptTraderId: "Trader ID",
    aiPromptTraderKey: "Trader Key",
    aiPromptRules: text("Request rules", "请求规则"),
    aiPromptRuleLines: chinese ? [
      "使用 PATCH，并设置 Content-Type: application/json 和 Authorization: Bearer <Trader Key>。",
      "请求体顶层必须是 { \"signal\": { ... } }。",
      "只能发送信号 JSON Schema 允许的字段；包含所有必填字段，并保持声明的 JSON 类型。",
      "把 Trader Key 视为机密：不要记录、暴露，或发送到这个 HIT 端点以外的任何地方。",
    ] : [
      "Send PATCH with Content-Type: application/json and Authorization: Bearer <Trader Key>.",
      "The top-level request body must be { \"signal\": { ... } }.",
      "Only send fields allowed by the Signal JSON Schema. Include every required field and preserve its declared JSON type.",
      "Treat the Trader Key as a secret: do not log, expose, or send it anywhere other than this HIT endpoint.",
    ],
    aiPromptSchema: text("Signal JSON Schema", "信号 JSON Schema"),
  }
}

function buildSignalCurl(endpoint: string, traderKey: string, payload: Record<string, unknown>): string {
  return [
    "curl --fail-with-body \\",
    "  -X PATCH '" + endpoint + "' \\",
    "  -H 'Authorization: Bearer " + traderKey + "' \\",
    "  -H 'Content-Type: application/json' \\",
    "  --data @- <<'JSON'",
    JSON.stringify(payload, null, 2),
    "JSON",
  ].join("\n")
}

function buildSignalIntegrationBrief(copy: SignalIntegrationCopy, trader: Trader, endpoint: string, traderKey: string, payload: Record<string, unknown>, schema?: JsonSchema): string {
  const curl = buildSignalCurl(endpoint, traderKey, payload)
  return `${copy.aiPromptIntro}

${copy.aiPromptEndpoint}: ${endpoint}
${copy.aiPromptTraderId}: ${trader.id}
${copy.aiPromptTraderKey}: ${traderKey}

${copy.aiPromptRules}:
${copy.aiPromptRuleLines.map(rule => "- " + rule).join("\n")}

curl:
\`\`\`bash
${curl}
\`\`\`

${copy.payload}:
\`\`\`json
${JSON.stringify(payload, null, 2)}
\`\`\`

${copy.aiPromptSchema}:
\`\`\`json
${JSON.stringify(schema ?? {}, null, 2)}
\`\`\``
}

async function copyText(value: string, successMessage: string, failureMessage: string) {
  try {
    await navigator.clipboard.writeText(value)
    toast.success(successMessage)
  } catch {
    toast.error(failureMessage)
  }
}

function CodeBlock({ value }: { value: string }) {
  return <pre className="m-0 max-h-96 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap break-words">{value}</pre>
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
