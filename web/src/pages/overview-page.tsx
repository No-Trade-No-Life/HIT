import { useQuery } from "@tanstack/react-query"
import { KeyRoundIcon, WorkflowIcon } from "lucide-react"
import { useNavigate } from "react-router-dom"

import { request } from "../lib/api"
import type { Copy } from "../lib/i18n"
import type { Credential, Trader } from "../lib/types"
import { EmptyState, TraderTable } from "../components/trader-ui"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

export function OverviewPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const traders = useQuery({ queryKey: ["traders", token], queryFn: () => request<Trader[]>("/api/v1/traders", token) })
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token) })
  if (traders.isPending || credentials.isPending) return <div className="grid gap-4 sm:grid-cols-3"><Skeleton className="h-24" /><Skeleton className="h-24" /><Skeleton className="h-24" /></div>

  const traderList = traders.data ?? []
  const credentialList = credentials.data ?? []
  const failed = traderList.filter(trader => trader.status === "failed").length
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.overview}</h1><p className="mt-1 text-sm text-muted-foreground">{t.usage}</p></div><div className="grid gap-4 sm:grid-cols-3"><Metric label={t.traders} value={traderList.length} /><Metric label={t.credentials} value={credentialList.length} /><Metric label={t.failed} value={failed} tone={failed ? "danger" : "default"} /></div><Card><CardHeader><CardTitle>{t.traders}</CardTitle><CardDescription>{t.updated}</CardDescription></CardHeader><CardContent>{traderList.length ? <TraderTable t={t} token={token} traders={traderList.slice(0, 5)} onSelect={id => navigate(`/traders/${id}`)} onChanged={onChanged} /> : <EmptyState title={t.noTraders} description={t.usage} icon={WorkflowIcon} />}</CardContent></Card>{!credentialList.length && <Card><CardHeader><CardTitle>{t.credentials}</CardTitle><CardDescription>{t.noCredentials}</CardDescription></CardHeader><CardContent><EmptyState title={t.noCredentials} description={t.usage} icon={KeyRoundIcon} /></CardContent></Card>}</div>
}

function Metric({ label, value, tone = "default" }: { label: string; value: number; tone?: "default" | "danger" }) {
  return <Card size="sm"><CardHeader><CardDescription>{label}</CardDescription><CardTitle className={tone === "danger" ? "text-destructive" : ""}>{value}</CardTitle></CardHeader></Card>
}
