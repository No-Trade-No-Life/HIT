import { useMemo, useState } from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { AuthMiniButton, useAuthMini } from "auth-mini-react-components"
import { ActivityIcon, BotIcon, CircleAlertIcon, CircleCheckIcon, KeyRoundIcon, LanguagesIcon, LayoutDashboardIcon, PencilIcon, PlusIcon, RefreshCwIcon, ShieldCheckIcon, Trash2Icon, WorkflowIcon } from "lucide-react"
import { toast } from "sonner"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Sidebar, SidebarContent, SidebarFooter, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarHeader, SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar"
import { Skeleton } from "@/components/ui/skeleton"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"

type Locale = "zh" | "en"
type Page = "overview" | "traders" | "credentials" | "linkit" | "admin"
type Me = { user_id: string; is_root: boolean; setup_required: boolean }
type Credential = { id: string; owner_id: string; exchange: string; label: string; created_at: number; updated_at: number }
type Trader = { id: string; owner_id: string; name: string; template_id: string; credential_id: string; params: Record<string, unknown>; signal: Record<string, unknown>; signal_token_prefix: string; enabled: boolean; status: string; last_run_at?: number; last_error?: string; updated_at: number }
type Run = { id: string; trader_id: string; status: string; summary?: string; started_at: number; finished_at: number }
type Template = { id: string; exchange: string; title: string; description: string; params_example: Record<string, unknown>; signal_example: Record<string, unknown> }
type LinkitSettings = { owner_id: string; recipient_username: string; configured: boolean; updated_at: number } | null

const copy = {
  zh: { overview: "总览", traders: "交易者", credentials: "交易凭证", linkit: "Linkit 通知", admin: "管理后台", createTrader: "新建交易者", createCredential: "新建凭证", noTraders: "还没有交易者", noCredentials: "还没有交易凭证", setup: "设为 Root 管理员", refresh: "刷新", enabled: "运行中", stopped: "已停止", failed: "失败", token: "信号 API Key", credential: "凭证", template: "策略模板", signal: "目标信号", params: "执行参数", runs: "最近运行", save: "保存", cancel: "取消", delete: "删除", language: "语言", root: "Root", owner: "所有者", status: "状态", updated: "更新于", detail: "运行详情", linkitDescription: "保存 Linkit Bot 的 sk- Token 后，交易执行失败会向你的 Linkit 用户名发送私信。", configured: "已配置", notConfigured: "未配置", signin: "正在验证登录状态…", usage: "外部信号使用以下密钥调用 PATCH /signal/v1/traders/{trader_id}，请求体为 { signal: {...} }。完整密钥只显示一次。" },
  en: { overview: "Overview", traders: "Traders", credentials: "Trading credentials", linkit: "Linkit notifications", admin: "Admin", createTrader: "Create trader", createCredential: "Create credential", noTraders: "No traders yet", noCredentials: "No trading credentials yet", setup: "Become root administrator", refresh: "Refresh", enabled: "Running", stopped: "Stopped", failed: "Failed", token: "Signal API key", credential: "Credential", template: "Strategy template", signal: "Target signal", params: "Execution parameters", runs: "Recent runs", save: "Save", cancel: "Cancel", delete: "Delete", language: "Language", root: "Root", owner: "Owner", status: "Status", updated: "Updated", detail: "Runtime details", linkitDescription: "Save a Linkit Bot sk- token to receive direct messages when one of your trader executions fails.", configured: "Configured", notConfigured: "Not configured", signin: "Checking your sign-in state…", usage: "External signals call PATCH /signal/v1/traders/{trader_id} with { signal: {...} }. The full key is shown once." },
} as const
type Copy = Record<keyof typeof copy.zh, string>

function request<T>(path: string, accessToken: string, init?: RequestInit): Promise<T> {
  return fetch(path, { ...init, headers: { "Content-Type": "application/json", Authorization: `Bearer ${accessToken}`, ...init?.headers } }).then(async response => {
    if (response.status === 204) return undefined as T
    const body = await response.json() as T & { error?: string }
    if (!response.ok) throw new Error(body.error ?? "Request failed")
    return body
  })
}

export default function App() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  const [locale, setLocale] = useState<Locale>(() => navigator.language.startsWith("zh") ? "zh" : "en")
  const [page, setPage] = useState<Page>("overview")
  const [selectedTrader, setSelectedTrader] = useState<string | null>(null)
  const t = copy[locale]
  const token = session?.accessToken ?? ""
  const queryClient = useQueryClient()
  const me = useQuery({ queryKey: ["me", token], queryFn: () => request<Me>("/api/v1/me", token), enabled: isAuthenticated })
  const traders = useQuery({ queryKey: ["traders", token], queryFn: () => request<Trader[]>("/api/v1/traders", token), enabled: isAuthenticated, refetchInterval: 10_000 })
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token), enabled: isAuthenticated })
  const templates = useQuery({ queryKey: ["templates"], queryFn: () => fetch("/api/templates").then(response => response.json() as Promise<Template[]>) })
  const refresh = () => void queryClient.invalidateQueries()

  if (!isReady || !isAuthenticated) return <div className="grid min-h-svh place-items-center text-sm text-muted-foreground">{t.signin}</div>
  if (me.isLoading) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>
  if (me.error) return <CenteredError error={me.error} />
  if (!me.data) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>

  const traderList = traders.data ?? []
  const credentialList = credentials.data ?? []
  const currentTrader = selectedTrader ? traderList.find(trader => trader.id === selectedTrader) ?? null : null
  const currentMe = me.data
  return <TooltipProvider>
    <SidebarProvider>
      <Sidebar collapsible="icon">
        <SidebarHeader className="px-3 py-4"><div className="flex items-center gap-2 font-semibold"><div className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">H</div><span className="group-data-[collapsible=icon]:hidden">HIT</span></div></SidebarHeader>
        <SidebarContent>
          <SidebarGroup><SidebarGroupLabel>{locale === "zh" ? "工作台" : "Workspace"}</SidebarGroupLabel><SidebarGroupContent><SidebarMenu>
            <NavItem icon={LayoutDashboardIcon} active={page === "overview"} onClick={() => { setPage("overview"); setSelectedTrader(null) }}>{t.overview}</NavItem>
            <NavItem icon={WorkflowIcon} active={page === "traders"} onClick={() => setPage("traders")}>{t.traders}</NavItem>
            <NavItem icon={KeyRoundIcon} active={page === "credentials"} onClick={() => setPage("credentials")}>{t.credentials}</NavItem>
            <NavItem icon={BotIcon} active={page === "linkit"} onClick={() => setPage("linkit")}>{t.linkit}</NavItem>
          </SidebarMenu></SidebarGroupContent></SidebarGroup>
          {currentMe.is_root && <SidebarGroup><SidebarGroupLabel>{locale === "zh" ? "系统" : "System"}</SidebarGroupLabel><SidebarGroupContent><SidebarMenu><NavItem icon={ShieldCheckIcon} active={page === "admin"} onClick={() => setPage("admin")}>{t.admin}</NavItem></SidebarMenu></SidebarGroupContent></SidebarGroup>}
        </SidebarContent>
        <SidebarFooter className="p-3"><AuthMiniButton lang={locale} variant="ghost" className="w-full justify-start" /></SidebarFooter>
      </Sidebar>
      <SidebarInset>
        <header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/85">
          <SidebarTrigger /><Separator orientation="vertical" className="h-5" />
          <div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{currentTrader ? currentTrader.name : t[page]}</p></div>
          {currentMe.is_root && <Badge variant="outline"><ShieldCheckIcon data-icon="inline-start" />{t.root}</Badge>}
          <Select value={locale} onValueChange={value => setLocale(value as Locale)}><SelectTrigger aria-label={t.language} size="sm"><LanguagesIcon /><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="zh">中文</SelectItem><SelectItem value="en">English</SelectItem></SelectGroup></SelectContent></Select>
          <Tooltip><TooltipTrigger render={<Button variant="ghost" size="icon-sm" onClick={refresh} aria-label={t.refresh} />}><RefreshCwIcon /></TooltipTrigger><TooltipContent>{t.refresh}</TooltipContent></Tooltip>
        </header>
        <main className="mx-auto w-full max-w-7xl p-4 md:p-6">
          {currentMe.setup_required ? <SetupPage token={token} onDone={refresh} label={t.setup} /> : currentTrader ? <TraderDetail trader={currentTrader} token={token} t={t} onBack={() => setSelectedTrader(null)} /> : <PageContent page={page} t={t} token={token} traders={traderList} credentials={credentialList} templates={templates.data ?? []} onSelectTrader={setSelectedTrader} onChanged={refresh} />}
        </main>
      </SidebarInset>
    </SidebarProvider>
  </TooltipProvider>
}

function NavItem({ icon: Icon, active, onClick, children }: { icon: typeof ActivityIcon; active: boolean; onClick: () => void; children: string }) { return <SidebarMenuItem><SidebarMenuButton isActive={active} onClick={onClick}><Icon /> <span>{children}</span></SidebarMenuButton></SidebarMenuItem> }
function CenteredError({ error }: { error: Error }) { return <div className="grid min-h-svh place-items-center p-6"><Alert className="max-w-lg"><CircleAlertIcon /><AlertTitle>HIT</AlertTitle><AlertDescription>{error.message}</AlertDescription></Alert></div> }

function SetupPage({ token, onDone, label }: { token: string; onDone: () => void; label: string }) {
  const mutation = useMutation({ mutationFn: () => request<Me>("/api/v1/setup", token, { method: "POST" }), onSuccess: () => { toast.success("Root user configured"); onDone() }, onError: showError })
  return <Card className="mx-auto mt-16 max-w-xl"><CardHeader><CardTitle>HIT initialization</CardTitle><CardDescription>The first authenticated user becomes the root user. This assigns HIT administration only; Auth Mini continues to own sign-in.</CardDescription></CardHeader><CardContent><Button onClick={() => mutation.mutate()} disabled={mutation.isPending}><ShieldCheckIcon data-icon="inline-start" />{label}</Button></CardContent></Card>
}

function PageContent({ page, t, token, traders, credentials, templates, onSelectTrader, onChanged }: { page: Page; t: Copy; token: string; traders: Trader[]; credentials: Credential[]; templates: Template[]; onSelectTrader: (id: string) => void; onChanged: () => void }) {
  if (page === "traders") return <TradersPage t={t} token={token} traders={traders} credentials={credentials} templates={templates} onSelect={onSelectTrader} onChanged={onChanged} />
  if (page === "credentials") return <CredentialsPage t={t} token={token} credentials={credentials} onChanged={onChanged} />
  if (page === "linkit") return <LinkitPage t={t} token={token} />
  if (page === "admin") return <AdminPage t={t} traders={traders} credentials={credentials} />
  return <Overview t={t} traders={traders} credentials={credentials} onTraders={() => onSelectTrader("")} />
}

function Overview({ t, traders, credentials, onTraders }: { t: Copy; traders: Trader[]; credentials: Credential[]; onTraders: () => void }) {
  const failed = traders.filter(trader => trader.status === "failed").length
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.overview}</h1><p className="mt-1 text-sm text-muted-foreground">{t.usage}</p></div><div className="grid gap-4 sm:grid-cols-3"><Metric label={t.traders} value={traders.length} /><Metric label={t.credentials} value={credentials.length} /><Metric label={t.failed} value={failed} tone={failed ? "danger" : "default"} /></div><Card><CardHeader><CardTitle>{t.traders}</CardTitle><CardDescription>{t.updated}</CardDescription></CardHeader><CardContent>{traders.length ? <TraderTable t={t} traders={traders.slice(0, 5)} onSelect={() => onTraders()} /> : <EmptyState title={t.noTraders} description={t.usage} icon={WorkflowIcon} />}</CardContent></Card></div>
}
function Metric({ label, value, tone = "default" }: { label: string; value: number; tone?: "default" | "danger" }) { return <Card size="sm"><CardHeader><CardDescription>{label}</CardDescription><CardTitle className={tone === "danger" ? "text-destructive" : ""}>{value}</CardTitle></CardHeader></Card> }

function TradersPage({ t, token, traders, credentials, templates, onSelect, onChanged }: { t: Copy; token: string; traders: Trader[]; credentials: Credential[]; templates: Template[]; onSelect: (id: string) => void; onChanged: () => void }) {
  const [open, setOpen] = useState(false)
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.traders}</h1><p className="mt-1 text-sm text-muted-foreground">{t.usage}</p></div><Button onClick={() => setOpen(true)} disabled={!credentials.length}><PlusIcon data-icon="inline-start" />{t.createTrader}</Button></div>{!credentials.length && <Alert><CircleAlertIcon /><AlertTitle>{t.credentials}</AlertTitle><AlertDescription>{localeText(t, "Create a credential before creating a trader.", "请先创建交易凭证，再创建交易者。")}</AlertDescription></Alert>}<Card><CardContent>{traders.length ? <TraderTable t={t} traders={traders} onSelect={onSelect} /> : <EmptyState title={t.noTraders} description={t.usage} icon={WorkflowIcon} />}</CardContent></Card><TraderDialog open={open} onOpenChange={setOpen} token={token} credentials={credentials} templates={templates} t={t} onCreated={onChanged} /></div>
}

function TraderTable({ t, traders, onSelect }: { t: Copy; traders: Trader[]; onSelect: (id: string) => void }) { return <Table><TableHeader><TableRow><TableHead>{t.traders}</TableHead><TableHead>{t.template}</TableHead><TableHead>{t.status}</TableHead><TableHead>{t.updated}</TableHead></TableRow></TableHeader><TableBody>{traders.map(trader => <TableRow key={trader.id} className="cursor-pointer" onClick={() => onSelect(trader.id)}><TableCell className="font-medium">{trader.name}</TableCell><TableCell className="max-w-72 truncate font-mono text-xs">{trader.template_id}</TableCell><TableCell><StatusBadge trader={trader} t={t} /></TableCell><TableCell className="text-muted-foreground">{formatTime(trader.updated_at)}</TableCell></TableRow>)}</TableBody></Table> }
function StatusBadge({ trader, t }: { trader: Trader; t: Copy }) { const failed = trader.status === "failed"; return <Badge variant={failed ? "destructive" : trader.enabled ? "default" : "outline"}>{failed ? <CircleAlertIcon data-icon="inline-start" /> : <CircleCheckIcon data-icon="inline-start" />}{failed ? t.failed : trader.enabled ? t.enabled : t.stopped}</Badge> }

function CredentialsPage({ t, token, credentials, onChanged }: { t: Copy; token: string; credentials: Credential[]; onChanged: () => void }) {
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<Credential | null>(null)
  const remove = useMutation({ mutationFn: (id: string) => request<void>(`/api/v1/credentials/${id}`, token, { method: "DELETE" }), onSuccess: () => { toast.success("Credential deleted"); onChanged() }, onError: showError })
  const close = () => { setOpen(false); setEditing(null) }
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.credentials}</h1><p className="mt-1 text-sm text-muted-foreground">Secrets are encrypted at rest and never returned to the browser.</p></div><Button onClick={() => { setEditing(null); setOpen(true) }}><PlusIcon data-icon="inline-start" />{t.createCredential}</Button></div><Card><CardContent>{credentials.length ? <Table><TableHeader><TableRow><TableHead>{t.credentials}</TableHead><TableHead>{localeText(t, "Exchange", "交易所")}</TableHead><TableHead>{t.updated}</TableHead><TableHead /></TableRow></TableHeader><TableBody>{credentials.map(credential => <TableRow key={credential.id}><TableCell className="font-medium">{credential.label}</TableCell><TableCell><Badge variant="outline">{credential.exchange.toUpperCase()}</Badge></TableCell><TableCell className="text-muted-foreground">{formatTime(credential.updated_at)}</TableCell><TableCell className="flex justify-end gap-1"><Button variant="ghost" size="icon-sm" onClick={() => { setEditing(credential); setOpen(true) }} aria-label={localeText(t, "Edit credential", "编辑凭证")}><PencilIcon /></Button><Button variant="ghost" size="icon-sm" onClick={() => remove.mutate(credential.id)} aria-label={t.delete}><Trash2Icon /></Button></TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.noCredentials} description={localeText(t, "Add a Binance, OKX, or CTPD credential. The secret is shown only to the executor.", "添加 Binance、OKX 或 CTPD 凭证。密钥仅在执行器进程内解密。")} icon={KeyRoundIcon} />}</CardContent></Card>{open && <CredentialDialog key={editing?.id ?? "new"} credential={editing} open={open} onOpenChange={next => { if (!next) close() }} token={token} t={t} onCreated={() => { close(); onChanged() }} />}</div>
}

function LinkitPage({ t, token }: { t: Copy; token: string }) {
  const query = useQuery({ queryKey: ["linkit", token], queryFn: () => request<LinkitSettings>("/api/v1/linkit", token) })
  const [recipient, setRecipient] = useState(""); const [botToken, setBotToken] = useState("")
  const mutation = useMutation({ mutationFn: () => request<LinkitSettings>("/api/v1/linkit", token, { method: "PUT", body: JSON.stringify({ recipient_username: recipient, bot_token: botToken }) }), onSuccess: () => { toast.success("Linkit configured"); void query.refetch(); setBotToken("") }, onError: showError })
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.linkit}</h1><p className="mt-1 text-sm text-muted-foreground">{t.linkitDescription}</p></div><Card className="max-w-2xl"><CardHeader><CardTitle>{query.data ? t.configured : t.notConfigured}</CardTitle><CardDescription>Linkit Bot API</CardDescription></CardHeader><CardContent><form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="linkit-recipient">Linkit username</FieldLabel><Input id="linkit-recipient" value={recipient} onChange={event => setRecipient(event.target.value)} placeholder={query.data?.recipient_username ?? "alice"} required /></Field><Field><FieldLabel htmlFor="linkit-token">Bot token</FieldLabel><Input id="linkit-token" type="password" value={botToken} onChange={event => setBotToken(event.target.value)} placeholder="sk-…" required /><FieldDescription>{localeText(t, "Create a bot in Linkit; its token is stored encrypted.", "在 Linkit 创建机器人；其 token 会加密保存。")}</FieldDescription></Field></FieldGroup><Button type="submit" disabled={mutation.isPending}>{t.save}</Button></form></CardContent></Card></div>
}

function AdminPage({ t, traders, credentials }: { t: Copy; traders: Trader[]; credentials: Credential[] }) { const owners = new Set([...traders, ...credentials].map(item => item.owner_id)); return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.admin}</h1><p className="mt-1 text-sm text-muted-foreground">Root can inspect all durable resources without receiving credential secrets or signal tokens.</p></div><div className="grid gap-4 sm:grid-cols-3"><Metric label={localeText(t, "Users", "用户")} value={owners.size} /><Metric label={t.traders} value={traders.length} /><Metric label={t.credentials} value={credentials.length} /></div><Card><CardHeader><CardTitle>{t.traders}</CardTitle><CardDescription>{localeText(t, "All users", "全部用户")}</CardDescription></CardHeader><CardContent><TraderTable t={t} traders={traders} onSelect={() => undefined} /></CardContent></Card></div> }

function TraderDialog({ open, onOpenChange, token, credentials, templates, t, onCreated }: { open: boolean; onOpenChange: (open: boolean) => void; token: string; credentials: Credential[]; templates: Template[]; t: Copy; onCreated: () => void }) {
  const [name, setName] = useState(""); const [credentialId, setCredentialId] = useState(""); const [templateId, setTemplateId] = useState(""); const [params, setParams] = useState("{}"); const [signal, setSignal] = useState("{}"); const [enabled, setEnabled] = useState(false); const [newToken, setNewToken] = useState<string | null>(null)
  const available = useMemo(() => templates.filter(template => !credentialId || credentials.find(credential => credential.id === credentialId)?.exchange === template.exchange), [templates, credentials, credentialId])
  const mutation = useMutation({ mutationFn: () => request<{ trader: Trader; signal_token: string }>("/api/v1/traders", token, { method: "POST", body: JSON.stringify({ name, credential_id: credentialId, template_id: templateId, params: JSON.parse(params), signal: JSON.parse(signal), enabled }) }), onSuccess: response => { setNewToken(response.signal_token); toast.success("Trader created"); onCreated() }, onError: showError })
  const selectTemplate = (id: string) => { const template = templates.find(item => item.id === id); setTemplateId(id); if (template) { setParams(JSON.stringify(template.params_example, null, 2)); setSignal(JSON.stringify(template.signal_example, null, 2)); } }
  return <Dialog open={open} onOpenChange={next => { onOpenChange(next); if (!next) setNewToken(null) }}><DialogContent className="max-w-2xl"><DialogHeader><DialogTitle>{t.createTrader}</DialogTitle><DialogDescription>{t.usage}</DialogDescription></DialogHeader>{newToken ? <Alert><KeyRoundIcon /><AlertTitle>{t.token}</AlertTitle><AlertDescription><code className="mt-2 block select-all break-all">{newToken}</code><p className="mt-2">{t.usage}</p></AlertDescription></Alert> : <form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="trader-name">{localeText(t, "Trader name", "交易者名称")}</FieldLabel><Input id="trader-name" value={name} onChange={event => setName(event.target.value)} required /></Field><Field><FieldLabel>{t.credential}</FieldLabel><Select value={credentialId} onValueChange={value => { if (value) { setCredentialId(value); setTemplateId("") } }}><SelectTrigger><SelectValue placeholder={t.credential} /></SelectTrigger><SelectContent><SelectGroup>{credentials.map(credential => <SelectItem value={credential.id} key={credential.id}>{credential.label} · {credential.exchange.toUpperCase()}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t.template}</FieldLabel><Select value={templateId} onValueChange={value => { if (value) selectTemplate(value) }}><SelectTrigger><SelectValue placeholder={t.template} /></SelectTrigger><SelectContent><SelectGroup>{available.map(template => <SelectItem value={template.id} key={template.id}>{template.title}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="params">{t.params}</FieldLabel><Textarea id="params" className="font-mono text-xs" value={params} onChange={event => setParams(event.target.value)} required /></Field><Field><FieldLabel htmlFor="signal">{t.signal}</FieldLabel><Textarea id="signal" className="font-mono text-xs" value={signal} onChange={event => setSignal(event.target.value)} required /></Field><Field orientation="horizontal"><Switch checked={enabled} onCheckedChange={setEnabled} id="trader-enabled" /><FieldLabel htmlFor="trader-enabled">{t.enabled}</FieldLabel></Field></FieldGroup><DialogFooter><Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t.cancel}</Button><Button type="submit" disabled={mutation.isPending || !templateId || !credentialId}>{t.createTrader}</Button></DialogFooter></form>}</DialogContent></Dialog>
}

function CredentialDialog({ open, onOpenChange, token, t, credential, onCreated }: { open: boolean; onOpenChange: (open: boolean) => void; token: string; t: Copy; credential: Credential | null; onCreated: () => void }) {
  const [exchange, setExchange] = useState(credential?.exchange ?? "binance"); const [label, setLabel] = useState(credential?.label ?? ""); const [apiKey, setApiKey] = useState(""); const [apiSecret, setApiSecret] = useState(""); const [passphrase, setPassphrase] = useState(""); const [baseUrl, setBaseUrl] = useState("")
  const mutation = useMutation({ mutationFn: () => { const secrets = exchange === "ctpd" ? { base_url: baseUrl, api_key: apiKey } : exchange === "okx" ? { api_key: apiKey, api_secret: apiSecret, passphrase } : { api_key: apiKey, api_secret: apiSecret }; return request<Credential>(credential ? `/api/v1/credentials/${credential.id}` : "/api/v1/credentials", token, { method: credential ? "PUT" : "POST", body: JSON.stringify({ exchange, label, secrets }) }) }, onSuccess: () => { toast.success("Credential saved"); onOpenChange(false); onCreated() }, onError: showError })
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent><DialogHeader><DialogTitle>{credential ? localeText(t, "Replace credential", "更新凭证") : t.createCredential}</DialogTitle><DialogDescription>{credential ? localeText(t, "Re-enter all secret fields to replace this credential.", "请重新填写所有密钥字段以更新此凭证。") : localeText(t, "HIT encrypts these secrets before SQLite writes them.", "HIT 在写入 SQLite 前加密这些密钥。")}</DialogDescription></DialogHeader><form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel>{localeText(t, "Exchange", "交易所")}</FieldLabel><Select value={exchange} onValueChange={value => { if (value && !credential) setExchange(value) }}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="binance">Binance UM Futures</SelectItem><SelectItem value="okx">OKX Swap</SelectItem><SelectItem value="ctpd">CTPD</SelectItem></SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="credential-label">{localeText(t, "Label", "名称")}</FieldLabel><Input id="credential-label" value={label} onChange={event => setLabel(event.target.value)} required /></Field>{exchange === "ctpd" && <Field><FieldLabel htmlFor="ctpd-url">CTPD base URL</FieldLabel><Input id="ctpd-url" type="url" value={baseUrl} onChange={event => setBaseUrl(event.target.value)} placeholder="https://ctp-01.ntnl.io" required /></Field>}<Field><FieldLabel htmlFor="api-key">API key</FieldLabel><Input id="api-key" value={apiKey} onChange={event => setApiKey(event.target.value)} required /></Field>{exchange !== "ctpd" && <Field><FieldLabel htmlFor="api-secret">API secret</FieldLabel><Input id="api-secret" type="password" value={apiSecret} onChange={event => setApiSecret(event.target.value)} required /></Field>}{exchange === "okx" && <Field><FieldLabel htmlFor="passphrase">Passphrase</FieldLabel><Input id="passphrase" type="password" value={passphrase} onChange={event => setPassphrase(event.target.value)} required /></Field>}</FieldGroup><DialogFooter><Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t.cancel}</Button><Button type="submit" disabled={mutation.isPending}>{t.save}</Button></DialogFooter></form></DialogContent></Dialog>
}

function TraderDetail({ trader, token, t, onBack }: { trader: Trader; token: string; t: Copy; onBack: () => void }) {
  const runs = useQuery({ queryKey: ["runs", trader.id, token], queryFn: () => request<Run[]>(`/api/v1/traders/${trader.id}/runs`, token), refetchInterval: 10_000 })
  const rotate = useMutation({ mutationFn: () => request<{ signal_token: string }>(`/api/v1/traders/${trader.id}/signal-token`, token, { method: "POST" }), onSuccess: response => { navigator.clipboard.writeText(response.signal_token).catch(() => undefined); toast.success("Signal token rotated and copied") }, onError: showError })
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><Button variant="ghost" size="sm" onClick={onBack}>← {t.traders}</Button><h1 className="mt-2 mb-0 text-2xl font-semibold tracking-tight">{trader.name}</h1><p className="mt-1 font-mono text-xs text-muted-foreground">{trader.id}</p></div><StatusBadge trader={trader} t={t} /></div>{trader.last_error && <Alert variant="destructive"><CircleAlertIcon /><AlertTitle>{t.failed}</AlertTitle><AlertDescription>{trader.last_error}</AlertDescription></Alert>}<div className="grid gap-4 lg:grid-cols-2"><Card><CardHeader><CardTitle>{t.params}</CardTitle><CardDescription>{t.credential}: {trader.credential_id}</CardDescription></CardHeader><CardContent><JsonBlock value={trader.params} /></CardContent></Card><Card><CardHeader><CardTitle>{t.signal}</CardTitle><CardDescription>{trader.signal_token_prefix}…</CardDescription></CardHeader><CardContent className="flex flex-col gap-4"><JsonBlock value={trader.signal} /><Button variant="outline" onClick={() => rotate.mutate()} disabled={rotate.isPending}><KeyRoundIcon data-icon="inline-start" />{t.token}</Button><p className="text-xs text-muted-foreground">{t.usage}</p></CardContent></Card></div><Card><CardHeader><CardTitle>{t.runs}</CardTitle><CardDescription>{t.detail}</CardDescription></CardHeader><CardContent>{runs.data?.length ? <Table><TableHeader><TableRow><TableHead>{t.status}</TableHead><TableHead>{t.updated}</TableHead><TableHead>{localeText(t, "Summary", "摘要")}</TableHead></TableRow></TableHeader><TableBody>{runs.data.map(run => <TableRow key={run.id}><TableCell><Badge variant={run.status === "failed" ? "destructive" : "default"}>{run.status}</Badge></TableCell><TableCell>{formatTime(run.started_at)}</TableCell><TableCell className="max-w-xl truncate font-mono text-xs">{run.summary}</TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.runs} description={localeText(t, "No execution has been recorded yet.", "尚未记录执行。") } icon={ActivityIcon} />}</CardContent></Card></div>
}

function JsonBlock({ value }: { value: Record<string, unknown> }) { return <pre className="m-0 max-h-72 overflow-auto rounded-md bg-muted p-3 font-mono text-xs leading-5 whitespace-pre-wrap break-words">{JSON.stringify(value, null, 2)}</pre> }
function EmptyState({ title, description, icon: Icon }: { title: string; description: string; icon: typeof ActivityIcon }) { return <Empty><EmptyHeader><EmptyMedia variant="icon"><Icon /></EmptyMedia><EmptyTitle>{title}</EmptyTitle><EmptyDescription>{description}</EmptyDescription></EmptyHeader><EmptyContent /></Empty> }
function formatTime(timestamp?: number) { return timestamp ? new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "medium" }).format(new Date(timestamp * 1000)) : "—" }
function showError(error: Error) { toast.error(error.message) }
function localeText(t: Copy, english: string, chinese: string) { return t.overview === "总览" ? chinese : english }
