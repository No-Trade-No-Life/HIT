import { useState } from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { toast } from "sonner"

import { request } from "../lib/api"
import { showError } from "../lib/format"
import type { Copy } from "../lib/i18n"
import type { LinkitSettings } from "../lib/types"
import { PageError } from "../components/trader-ui"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Field, FieldDescription, FieldGroup, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"

export function LinkitPage({ token, t }: { token: string; t: Copy }) {
  const [recipient, setRecipient] = useState("")
  const [botToken, setBotToken] = useState("")
  const settings = useQuery({ queryKey: ["linkit", token], queryFn: () => request<LinkitSettings>("/api/v1/linkit", token) })
  const mutation = useMutation({ mutationFn: () => request<LinkitSettings>("/api/v1/linkit", token, { method: "PUT", body: JSON.stringify({ recipient_username: recipient, bot_token: botToken }) }), onSuccess: () => { toast.success("Linkit configured"); void settings.refetch(); setBotToken("") }, onError: showError })
  if (settings.error) return <PageError error={settings.error} />
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.linkit}</h1><p className="mt-1 text-sm text-muted-foreground">{t.linkitDescription}</p></div><Card className="max-w-2xl"><CardHeader><CardTitle>{settings.data ? t.configured : t.notConfigured}</CardTitle><CardDescription>Linkit Bot API</CardDescription></CardHeader><CardContent><form className="flex flex-col gap-5" onSubmit={event => { event.preventDefault(); mutation.mutate() }}><FieldGroup><Field><FieldLabel htmlFor="linkit-recipient">Linkit username</FieldLabel><Input id="linkit-recipient" value={recipient} onChange={event => setRecipient(event.target.value)} placeholder={settings.data?.recipient_username ?? "alice"} required /></Field><Field><FieldLabel htmlFor="linkit-token">Bot token</FieldLabel><Input id="linkit-token" type="password" value={botToken} onChange={event => setBotToken(event.target.value)} placeholder="sk-…" required /><FieldDescription>{t.linkitDescription}</FieldDescription></Field></FieldGroup><Button type="submit" disabled={mutation.isPending}>{t.save}</Button></form></CardContent></Card></div>
}
