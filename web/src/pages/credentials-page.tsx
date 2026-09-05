import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { KeyRoundIcon, PencilIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { toast } from "sonner"

import { request } from "../lib/api"
import { formatTime, showError } from "../lib/format"
import { localeText, type Copy } from "../lib/i18n"
import type { Credential } from "../lib/types"
import { EmptyState, PageError } from "../components/trader-ui"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function CredentialsPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const [open, setOpen] = useState(false)
  const [editing, setEditing] = useState<Credential | null>(null)
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token) })
  const remove = useMutation({ mutationFn: (id: string) => request<void>(`/api/v1/credentials/${id}`, token, { method: "DELETE" }), onSuccess: () => { toast.success("Credential deleted"); onChanged() }, onError: showError })
  if (credentials.error) return <PageError error={credentials.error} />

  const credentialList = credentials.data ?? []
  const close = () => { setOpen(false); setEditing(null) }
  return <div className="flex flex-col gap-6"><div className="flex flex-wrap items-end justify-between gap-3"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.credentials}</h1><p className="mt-1 text-sm text-muted-foreground">{localeText(t, "Secrets are encrypted at rest and never returned to the browser.", "密钥会加密保存，且永远不会返回浏览器。")}</p></div><Button onClick={() => { setEditing(null); setOpen(true) }}><PlusIcon data-icon="inline-start" />{t.createCredential}</Button></div><Card><CardContent>{credentials.isPending ? <div className="py-8 text-sm text-muted-foreground">{t.refresh}</div> : credentialList.length ? <Table><TableHeader><TableRow><TableHead>{t.credentials}</TableHead><TableHead>{localeText(t, "Exchange", "交易所")}</TableHead><TableHead>{t.updated}</TableHead><TableHead /></TableRow></TableHeader><TableBody>{credentialList.map(credential => <TableRow key={credential.id}><TableCell className="font-medium">{credential.label}</TableCell><TableCell>{credential.exchange.toUpperCase()}</TableCell><TableCell className="text-muted-foreground">{formatTime(credential.updated_at)}</TableCell><TableCell className="flex justify-end gap-1"><Button variant="ghost" size="icon-sm" onClick={() => { setEditing(credential); setOpen(true) }} aria-label={localeText(t, "Edit credential", "编辑凭证")}><PencilIcon /></Button><Button variant="ghost" size="icon-sm" onClick={() => remove.mutate(credential.id)} aria-label={t.delete}><Trash2Icon /></Button></TableCell></TableRow>)}</TableBody></Table> : <EmptyState title={t.noCredentials} description={localeText(t, "Add a Binance, OKX, or CTPD credential.", "添加 Binance、OKX 或 CTPD 凭证。")} icon={KeyRoundIcon} />}</CardContent></Card>{open && <CredentialDialog credential={editing} open={open} onOpenChange={next => { if (!next) close() }} token={token} t={t} onCreated={() => { close(); onChanged() }} />}</div>
}

function CredentialDialog({ open, onOpenChange, token, t, credential, onCreated }: { open: boolean; onOpenChange: (open: boolean) => void; token: string; t: Copy; credential: Credential | null; onCreated: () => void }) {
  const [exchange, setExchange] = useState(credential?.exchange ?? "binance")
  const [label, setLabel] = useState(credential?.label ?? "")
  const [apiKey, setApiKey] = useState("")
  const [apiSecret, setApiSecret] = useState("")
  const [passphrase, setPassphrase] = useState("")
  const [baseUrl, setBaseUrl] = useState("")
  const mutation = useMutation({ mutationFn: () => { const secrets = exchange === "ctpd" ? { base_url: baseUrl, api_key: apiKey } : exchange === "okx" ? { api_key: apiKey, api_secret: apiSecret, passphrase } : { api_key: apiKey, api_secret: apiSecret }; return request<Credential>(credential ? `/api/v1/credentials/${credential.id}` : "/api/v1/credentials", token, { method: credential ? "PUT" : "POST", body: JSON.stringify({ exchange, label, secrets }) }) }, onSuccess: () => { toast.success("Credential saved"); onOpenChange(false); onCreated() }, onError: showError })
  return <Dialog open={open} onOpenChange={onOpenChange}><DialogContent><DialogHeader><DialogTitle>{credential ? localeText(t, "Replace credential", "更新凭证") : t.createCredential}</DialogTitle><DialogDescription>{credential ? localeText(t, "Re-enter all secret fields to replace this credential.", "请重新填写所有密钥字段以更新此凭证。") : localeText(t, "HIT encrypts these secrets before SQLite writes them.", "HIT 在写入 SQLite 前加密这些密钥。")}</DialogDescription></DialogHeader><form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel>{localeText(t, "Exchange", "交易所")}</FieldLabel><Select value={exchange} onValueChange={value => { if (value && !credential) setExchange(value) }}><SelectTrigger><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="binance">Binance UM Futures</SelectItem><SelectItem value="okx">OKX Swap</SelectItem><SelectItem value="ctpd">CTPD</SelectItem></SelectGroup></SelectContent></Select></Field><Field><FieldLabel htmlFor="credential-label">{localeText(t, "Label", "名称")}</FieldLabel><Input id="credential-label" value={label} onChange={event => setLabel(event.target.value)} required /></Field>{exchange === "ctpd" && <Field><FieldLabel htmlFor="ctpd-url">CTPD base URL</FieldLabel><Input id="ctpd-url" type="url" value={baseUrl} onChange={event => setBaseUrl(event.target.value)} placeholder="https://ctp-01.ntnl.io" required /></Field>}<Field><FieldLabel htmlFor="api-key">API key</FieldLabel><Input id="api-key" value={apiKey} onChange={event => setApiKey(event.target.value)} required /></Field>{exchange !== "ctpd" && <Field><FieldLabel htmlFor="api-secret">API secret</FieldLabel><Input id="api-secret" type="password" value={apiSecret} onChange={event => setApiSecret(event.target.value)} required /></Field>}{exchange === "okx" && <Field><FieldLabel htmlFor="passphrase">Passphrase</FieldLabel><Input id="passphrase" type="password" value={passphrase} onChange={event => setPassphrase(event.target.value)} required /></Field>}<FieldDescription>{localeText(t, "Secrets are never shown after saving.", "保存后不会再次显示密钥。")}</FieldDescription></FieldGroup><DialogFooter><Button type="button" variant="outline" onClick={() => onOpenChange(false)}>{t.cancel}</Button><Button type="submit" disabled={mutation.isPending}>{t.save}</Button></DialogFooter></form></DialogContent></Dialog>
}
