import { useMemo, useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { CircleAlertIcon, KeyRoundIcon, PlusIcon, WorkflowIcon } from "lucide-react"
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

export function TradersPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const [open, setOpen] = useState(false)
  const traders = useQuery({ queryKey: ["traders", token], queryFn: () => request<Trader[]>("/api/v1/traders", token) })
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token) })
  const templates = useQuery({ queryKey: ["templates"], queryFn: () => fetch("/api/templates").then(response => response.json() as Promise<Template[]>) })
  if (traders.error) return <PageError error={traders.error} />
  if (credentials.error) return <PageError error={credentials.error} />

  const traderList = traders.data ?? []
  const credentialList = credentials.data ?? []
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.traders}</h1><p className="mt-1 text-sm text-muted-foreground">{t.usage}</p></div><Button onClick={() => setOpen(true)} disabled={!credentialList.length}><PlusIcon data-icon="inline-start" />{t.createTrader}</Button></div>{!credentialList.length && <Alert><CircleAlertIcon /><AlertTitle>{t.credentials}</AlertTitle><AlertDescription>{localeText(t, "Create a credential before creating a trader.", "请先创建交易凭证，再创建交易者。")}</AlertDescription></Alert>}<Card><CardContent>{traders.isPending ? <div className="py-8 text-sm text-muted-foreground">{t.refresh}</div> : traderList.length ? <TraderTable t={t} token={token} traders={traderList} onSelect={id => navigate(`/traders/${id}`)} onChanged={onChanged} /> : <EmptyState title={t.noTraders} description={t.usage} icon={WorkflowIcon} />}</CardContent></Card><TraderDialog open={open} onOpenChange={setOpen} token={token} credentials={credentialList} templates={templates.data ?? []} t={t} onCreated={onChanged} /></div>
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
  return <Dialog open={open} onOpenChange={next => { onOpenChange(next); if (!next) setNewToken(null) }}><DialogContent className="max-w-2xl"><DialogHeader><DialogTitle>{t.createTrader}</DialogTitle><DialogDescription>{t.usage}</DialogDescription></DialogHeader>{newToken ? <Alert><KeyRoundIcon /><AlertTitle>{t.token}</AlertTitle><AlertDescription><code className="mt-2 block select-all break-all">{newToken}</code><p className="mt-2">{t.usage}</p></AlertDescription></Alert> : <form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="trader-name">{localeText(t, "Trader name", "交易者名称")}</FieldLabel><Input id="trader-name" value={name} onChange={event => setName(event.target.value)} required /></Field><Field><FieldLabel>{t.credential}</FieldLabel><Select value={credentialId} onValueChange={value => { if (value) { setCredentialId(value); setTemplateId("") } }}><SelectTrigger><SelectValue placeholder={t.credential} /></SelectTrigger><SelectContent><SelectGroup>{credentials.map(credential => <SelectItem value={credential.id} key={credential.id}>{credential.label} · {credential.exchange.toUpperCase()}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel>{t.template}</FieldLabel><Select value={templateId} onValueChange={value => { if (value) selectTemplate(value) }}><SelectTrigger><SelectValue placeholder={t.template} /></SelectTrigger><SelectContent><SelectGroup>{available.map(template => <SelectItem value={template.id} key={template.id}>{template.title}</SelectItem>)}</SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="params">{t.params}</FieldLabel><Textarea id="params" className="font-mono text-xs" value={params} onChange={event => setParams(event.target.value)} required /></Field><Field><FieldLabel htmlFor="signal">{t.signal}</FieldLabel><Textarea id="signal" className="font-mono text-xs" value={signal} onChange={event => setSignal(event.target.value)} required /></Field><Field orientation="horizontal"><Switch checked={enabled} onCheckedChange={setEnabled} id="trader-enabled" /><FieldLabel htmlFor="trader-enabled">{t.enabled}</FieldLabel></Field></FieldGroup><DialogFooter><Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t.cancel}</Button><Button type="submit" disabled={mutation.isPending || !templateId || !credentialId}>{t.createTrader}</Button></DialogFooter></form>}</DialogContent></Dialog>
}
