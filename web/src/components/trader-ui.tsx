import { useMutation } from "@tanstack/react-query"
import { LinkitUserInfo } from "linkit-react-components"
import { ActivityIcon, CircleAlertIcon, CircleCheckIcon, type LucideIcon } from "lucide-react"
import { toast } from "sonner"

import { request } from "../lib/api"
import { formatTime, showError } from "../lib/format"
import type { Copy } from "../lib/i18n"
import type { Trader } from "../lib/types"
import { Badge } from "@/components/ui/badge"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Switch } from "@/components/ui/switch"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function TraderTable({ t, token, traders, onSelect, onChanged, showOwner = false }: { t: Copy; token: string; traders: Trader[]; onSelect: (id: string) => void; onChanged: () => void; showOwner?: boolean }) {
  return <Table><TableHeader><TableRow><TableHead>{t.traders}</TableHead>{showOwner && <TableHead>{t.owner}</TableHead>}<TableHead>{t.template}</TableHead><TableHead>{t.status}</TableHead><TableHead>{t.execution}</TableHead><TableHead>{t.updated}</TableHead></TableRow></TableHeader><TableBody>{traders.map(trader => <TableRow key={trader.id} className="cursor-pointer" onClick={() => onSelect(trader.id)}><TableCell className="font-medium">{trader.name}</TableCell>{showOwner && <TableCell className="min-w-44"><LinkitUserInfo userId={trader.owner_id} compact /></TableCell>}<TableCell className="max-w-72 truncate font-mono text-xs">{trader.template_id}</TableCell><TableCell><StatusBadge trader={trader} t={t} /></TableCell><TableCell><TraderEnabledSwitch trader={trader} token={token} t={t} onChanged={onChanged} /></TableCell><TableCell className="text-muted-foreground">{formatTime(trader.updated_at)}</TableCell></TableRow>)}</TableBody></Table>
}

export function TraderEnabledSwitch({ trader, token, t, onChanged }: { trader: Trader; token: string; t: Copy; onChanged: () => void }) {
  const mutation = useMutation({ mutationFn: (enabled: boolean) => request<Trader>(`/api/v1/traders/${trader.id}/enabled`, token, { method: "PATCH", body: JSON.stringify({ enabled }) }), onSuccess: (_, enabled) => { toast.success(enabled ? t.traderEnabled : t.traderStopped); onChanged() }, onError: showError })
  return <div className="flex items-center justify-end gap-2" onClick={event => event.stopPropagation()}><span className="text-xs text-muted-foreground">{trader.enabled ? t.enabled : t.stopped}</span><Switch size="sm" checked={trader.enabled} onCheckedChange={enabled => mutation.mutate(enabled)} disabled={mutation.isPending} aria-label={trader.enabled ? t.disableTrader : t.enableTrader} /></div>
}

export function StatusBadge({ trader, t }: { trader: Trader; t: Copy }) {
  const failed = trader.status === "failed"
  return <Badge variant={failed ? "destructive" : trader.enabled ? "default" : "outline"}>{failed ? <CircleAlertIcon data-icon="inline-start" /> : <CircleCheckIcon data-icon="inline-start" />}{failed ? t.failed : trader.enabled ? t.enabled : t.stopped}</Badge>
}

export function EmptyState({ title, description, icon: Icon = ActivityIcon }: { title: string; description: string; icon?: LucideIcon }) {
  return <Empty><EmptyHeader><EmptyMedia variant="icon"><Icon /></EmptyMedia><EmptyTitle>{title}</EmptyTitle><EmptyDescription>{description}</EmptyDescription></EmptyHeader><EmptyContent /></Empty>
}

export function PageError({ error }: { error: Error }) {
  return <div className="grid min-h-48 place-items-center p-6"><p className="text-sm text-destructive">{error.message}</p></div>
}
