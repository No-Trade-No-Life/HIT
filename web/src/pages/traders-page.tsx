import { useMemo, useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { ChevronLeftIcon, ChevronRightIcon, CircleAlertIcon, KeyRoundIcon, PlusIcon, RotateCcwIcon, WorkflowIcon } from "lucide-react"
import { useNavigate } from "react-router-dom"
import { toast } from "sonner"

import { request } from "../lib/api"
import { showError } from "../lib/format"
import { localeText, type Copy } from "../lib/i18n"
import type { Credential, Template, Trader } from "../lib/types"
import { EmptyState, PageError, TraderTable } from "../components/trader-ui"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"

type TraderStatusFilter = "all" | "running" | "stopped" | "failed"
type SignalFilter = "all" | "received" | "missing"
type TraderSort = "created-desc" | "created-asc" | "signal-desc" | "signal-asc" | "name-asc"

const pageSizes = [10, 25, 50] as const

export function TradersPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const traders = useQuery({ queryKey: ["traders", token], queryFn: () => request<Trader[]>("/api/v1/traders", token), refetchInterval: 15_000 })
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token) })
  const templates = useQuery({ queryKey: ["templates"], queryFn: () => fetch("/api/templates").then(response => response.json() as Promise<Template[]>) })
  if (traders.error) return <PageError error={traders.error} />
  if (credentials.error) return <PageError error={credentials.error} />

  const traderList = traders.data ?? []
  const credentialList = credentials.data ?? []
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.traders}</h1><p className="mt-1 text-sm text-muted-foreground">{t.usage}</p></div><Button onClick={() => setOpen(true)} disabled={!credentialList.length}><PlusIcon data-icon="inline-start" />{t.createTrader}</Button></div>{!credentialList.length && <Alert><CircleAlertIcon /><AlertTitle>{t.credentials}</AlertTitle><AlertDescription>{localeText(t, "Create a credential before creating a trader.", "请先创建交易凭证，再创建交易者。")}</AlertDescription></Alert>}<Card><CardContent>{traders.isPending ? <div className="py-8 text-sm text-muted-foreground">{t.refresh}</div> : traderList.length ? <TraderList t={t} token={token} traders={traderList} onSelect={id => navigate(`/traders/${id}`)} onChanged={onChanged} /> : <EmptyState title={t.noTraders} description={t.usage} icon={WorkflowIcon} />}</CardContent></Card><TraderDialog open={open} onOpenChange={setOpen} token={token} credentials={credentialList} templates={templates.data ?? []} t={t} onCreated={onChanged} /></div>
}

function TraderList({ t, token, traders, onSelect, onChanged }: { t: Copy; token: string; traders: Trader[]; onSelect: (id: string) => void; onChanged: () => void }) {
  const [query, setQuery] = useState("")
  const [status, setStatus] = useState<TraderStatusFilter>("all")
  const [signal, setSignal] = useState<SignalFilter>("all")
  const [sort, setSort] = useState<TraderSort>("created-desc")
  const [pageSize, setPageSize] = useState<(typeof pageSizes)[number]>(pageSizes[0])
  const [page, setPage] = useState(1)
  const locale = localeText(t, "en", "zh-CN")
  const filteredTraders = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase(locale)
    return traders.filter(trader => {
      const matchesQuery = !normalizedQuery || trader.name.toLocaleLowerCase(locale).includes(normalizedQuery) || trader.template_id.toLocaleLowerCase(locale).includes(normalizedQuery)
      const matchesStatus = status === "all" || traderDisplayStatus(trader) === status
      const matchesSignal = signal === "all" || (signal === "received" ? trader.last_signal_at != null : trader.last_signal_at == null)
      return matchesQuery && matchesStatus && matchesSignal
    }).sort((left, right) => compareTraders(left, right, sort, locale))
  }, [locale, query, signal, sort, status, traders])
  const pageCount = Math.max(1, Math.ceil(filteredTraders.length / pageSize))
  const currentPage = Math.min(page, pageCount)
  const firstItem = filteredTraders.length ? (currentPage - 1) * pageSize + 1 : 0
  const pageTraders = filteredTraders.slice(firstItem - 1, firstItem - 1 + pageSize)
  const lastItem = firstItem + pageTraders.length - 1
  const controlsAreDefault = !query && status === "all" && signal === "all" && sort === "created-desc" && pageSize === pageSizes[0]
  const resetControls = () => { setQuery(""); setStatus("all"); setSignal("all"); setSort("created-desc"); setPageSize(pageSizes[0]); setPage(1) }

  return <div className="flex flex-col gap-5"><form onSubmit={event => event.preventDefault()}><FieldGroup className="!flex-row flex-wrap items-end !gap-3"><Field className="!w-auto min-w-56 flex-1"><FieldLabel htmlFor="trader-search">{localeText(t, "Search", "搜索")}</FieldLabel><Input id="trader-search" value={query} onChange={event => { setQuery(event.target.value); setPage(1) }} placeholder={localeText(t, "Name or strategy template", "名称或策略模板")} /></Field><Field className="!w-auto min-w-32"><FieldLabel htmlFor="trader-status-filter">{t.status}</FieldLabel><Select value={status} onValueChange={value => { setStatus(value as TraderStatusFilter); setPage(1) }}><SelectTrigger id="trader-status-filter" className="!w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="all">{localeText(t, "All statuses", "全部状态")}</SelectItem><SelectItem value="running">{t.enabled}</SelectItem><SelectItem value="stopped">{t.stopped}</SelectItem><SelectItem value="failed">{t.failed}</SelectItem></SelectGroup></SelectContent></Select></Field><Field className="!w-auto min-w-40"><FieldLabel htmlFor="trader-signal-filter">{localeText(t, "External signal", "外部信号")}</FieldLabel><Select value={signal} onValueChange={value => { setSignal(value as SignalFilter); setPage(1) }}><SelectTrigger id="trader-signal-filter" className="!w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="all">{localeText(t, "All signals", "全部信号")}</SelectItem><SelectItem value="received">{localeText(t, "Received", "已接收")}</SelectItem><SelectItem value="missing">{localeText(t, "Not received", "未接收")}</SelectItem></SelectGroup></SelectContent></Select></Field><Field className="!w-auto min-w-52"><FieldLabel htmlFor="trader-sort">{localeText(t, "Sort", "排序")}</FieldLabel><Select value={sort} onValueChange={value => { setSort(value as TraderSort); setPage(1) }}><SelectTrigger id="trader-sort" className="!w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="created-desc">{localeText(t, "Created: newest first", "创建时间：最新优先")}</SelectItem><SelectItem value="created-asc">{localeText(t, "Created: oldest first", "创建时间：最早优先")}</SelectItem><SelectItem value="signal-desc">{localeText(t, "Last signal: newest first", "最后接收信号：最新优先")}</SelectItem><SelectItem value="signal-asc">{localeText(t, "Last signal: oldest first", "最后接收信号：最早优先")}</SelectItem><SelectItem value="name-asc">{localeText(t, "Name: A to Z", "名称：A 到 Z")}</SelectItem></SelectGroup></SelectContent></Select></Field><Field className="!w-auto min-w-28"><FieldLabel htmlFor="trader-page-size">{localeText(t, "Per page", "每页")}</FieldLabel><Select value={String(pageSize)} onValueChange={value => { setPageSize(Number(value) as (typeof pageSizes)[number]); setPage(1) }}><SelectTrigger id="trader-page-size" className="!w-full"><SelectValue /></SelectTrigger><SelectContent><SelectGroup>{pageSizes.map(size => <SelectItem value={String(size)} key={size}>{size}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Button type="button" variant="ghost" size="sm" onClick={resetControls} disabled={controlsAreDefault}><RotateCcwIcon data-icon="inline-start" />{localeText(t, "Reset", "重置")}</Button></FieldGroup></form>{filteredTraders.length ? <><TraderTable t={t} token={token} traders={pageTraders} onSelect={onSelect} onChanged={onChanged} /><nav className="flex flex-wrap items-center justify-between gap-3" aria-label={localeText(t, "Trader list pagination", "交易者列表分页")}><p className="text-sm text-muted-foreground">{localeText(t, `Showing ${firstItem}-${lastItem} of ${filteredTraders.length}`, `显示第 ${firstItem}-${lastItem} 项，共 ${filteredTraders.length} 项`)}</p><div className="flex items-center gap-2"><Button type="button" variant="outline" size="sm" onClick={() => setPage(currentPage - 1)} disabled={currentPage === 1}><ChevronLeftIcon data-icon="inline-start" />{localeText(t, "Previous", "上一页")}</Button><span className="text-sm tabular-nums text-muted-foreground">{localeText(t, `Page ${currentPage} of ${pageCount}`, `第 ${currentPage} / ${pageCount} 页`)}</span><Button type="button" variant="outline" size="sm" onClick={() => setPage(currentPage + 1)} disabled={currentPage === pageCount}>{localeText(t, "Next", "下一页")}<ChevronRightIcon data-icon="inline-end" /></Button></div></nav></> : <EmptyState title={localeText(t, "No matching traders", "没有匹配的交易者")} description={localeText(t, "Adjust the filters or reset them to see every trader.", "请调整筛选条件，或重置以查看全部交易者。")} icon={WorkflowIcon} />}</div>
}

function traderDisplayStatus(trader: Trader): Exclude<TraderStatusFilter, "all"> {
  if (trader.status === "failed") return "failed"
  return trader.enabled ? "running" : "stopped"
}

function compareTraders(left: Trader, right: Trader, sort: TraderSort, locale: string) {
  const tieBreak = () => right.id.localeCompare(left.id, locale)
  if (sort === "name-asc") return left.name.localeCompare(right.name, locale) || tieBreak()
  const [field, direction] = sort.split("-") as ["created" | "signal", "asc" | "desc"]
  const leftValue = field === "created" ? left.created_at : left.last_signal_at
  const rightValue = field === "created" ? right.created_at : right.last_signal_at
  if (leftValue == null) return rightValue == null ? tieBreak() : 1
  if (rightValue == null) return -1
  return (direction === "asc" ? leftValue - rightValue : rightValue - leftValue) || tieBreak()
}

function TraderDialog({ open, onOpenChange, token, credentials, templates, t, onCreated }: { open: boolean; onOpenChange: (open: boolean) => void; token: string; credentials: Credential[]; templates: Template[]; t: Copy; onCreated: () => void }) {
  const [name, setName] = useState("")
  const [credentialId, setCredentialId] = useState("")
  const [templateId, setTemplateId] = useState("")
  const [params, setParams] = useState("{}")
  const [signal, setSignal] = useState("{}")
  const [enabled, setEnabled] = useState(false)
  const [newToken, setNewToken] = useState<string | null>(null)
  const available = useMemo(() => templates.filter(template => !credentialId || credentials.find(credential => credential.id === credentialId)?.exchange === template.exchange), [templates, credentials, credentialId])
  const mutation = useMutation({ mutationFn: () => request<{ trader: Trader; signal_token: string }>("/api/v1/traders", token, { method: "POST", body: JSON.stringify({ name, credential_id: credentialId, template_id: templateId, params: JSON.parse(params), signal: JSON.parse(signal), enabled }) }), onSuccess: response => { setNewToken(response.signal_token); toast.success("Trader created"); onCreated() }, onError: showError })
  const selectTemplate = (id: string) => { const template = templates.find(item => item.id === id); setTemplateId(id); if (template) { setParams(JSON.stringify(template.params_example, null, 2)); setSignal(JSON.stringify(template.signal_example, null, 2)); } }
  return <Dialog open={open} onOpenChange={next => { onOpenChange(next); if (!next) setNewToken(null) }}><DialogContent className="max-w-2xl"><DialogHeader><DialogTitle>{t.createTrader}</DialogTitle><DialogDescription>{t.usage}</DialogDescription></DialogHeader>{newToken ? <Alert><KeyRoundIcon /><AlertTitle>{t.token}</AlertTitle><AlertDescription><code className="mt-2 block select-all break-all">{newToken}</code><p className="mt-2">{t.usage}</p></AlertDescription></Alert> : <form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="trader-name">{localeText(t, "Trader name", "交易者名称")}</FieldLabel><Input id="trader-name" value={name} onChange={event => setName(event.target.value)} required /></Field><Field><FieldLabel>{t.credential}</FieldLabel><Select value={credentialId} onValueChange={value => { if (value) { setCredentialId(value); setTemplateId("") } }}><SelectTrigger><SelectValue placeholder={t.credential} /></SelectTrigger><SelectContent><SelectGroup>{credentials.map(credential => <SelectItem value={credential.id} key={credential.id}>{credential.label} · {credential.exchange.toUpperCase()}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t.template}</FieldLabel><Select value={templateId} onValueChange={value => { if (value) selectTemplate(value) }}><SelectTrigger><SelectValue placeholder={t.template} /></SelectTrigger><SelectContent><SelectGroup>{available.map(template => <SelectItem value={template.id} key={template.id}>{template.name}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="params">{t.params}</FieldLabel><Textarea id="params" className="font-mono text-xs" value={params} onChange={event => setParams(event.target.value)} required /></Field><Field><FieldLabel htmlFor="signal">{t.signal}</FieldLabel><Textarea id="signal" className="font-mono text-xs" value={signal} onChange={event => setSignal(event.target.value)} required /></Field><Field orientation="horizontal"><Switch checked={enabled} onCheckedChange={setEnabled} id="trader-enabled" /><FieldLabel htmlFor="trader-enabled">{t.enabled}</FieldLabel></Field></FieldGroup><DialogFooter><Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t.cancel}</Button><Button type="submit" disabled={mutation.isPending || !templateId || !credentialId}>{t.createTrader}</Button></DialogFooter></form>}</DialogContent></Dialog>
}
