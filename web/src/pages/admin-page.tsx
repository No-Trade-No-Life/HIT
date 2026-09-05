import { useQuery } from "@tanstack/react-query"
import { useNavigate } from "react-router-dom"

import { request } from "../lib/api"
import { localeText, type Copy } from "../lib/i18n"
import type { Credential, Trader } from "../lib/types"
import { PageError, TraderTable } from "../components/trader-ui"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

export function AdminPage({ token, t, onChanged }: { token: string; t: Copy; onChanged: () => void }) {
  const navigate = useNavigate()
  const traders = useQuery({ queryKey: ["traders", token], queryFn: () => request<Trader[]>("/api/v1/traders", token) })
  const credentials = useQuery({ queryKey: ["credentials", token], queryFn: () => request<Credential[]>("/api/v1/credentials", token) })
  if (traders.error) return <PageError error={traders.error} />
  if (credentials.error) return <PageError error={credentials.error} />
  if (traders.isPending || credentials.isPending) return <Skeleton className="h-72" />

  const traderList = traders.data ?? []
  const owners = new Set([...traderList, ...(credentials.data ?? [])].map(item => item.owner_id))
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.admin}</h1><p className="mt-1 text-sm text-muted-foreground">{localeText(t, "Inspect every user resource without receiving credential secrets or signal tokens.", "查看所有用户的资源，但不会接收凭证密钥或信号令牌。")}</p></div><div className="grid gap-4 sm:grid-cols-3"><Metric label={localeText(t, "Users", "用户")} value={owners.size} /><Metric label={t.traders} value={traderList.length} /><Metric label={t.credentials} value={(credentials.data ?? []).length} /></div><Card><CardHeader><CardTitle>{t.traders}</CardTitle><CardDescription>{localeText(t, "All users", "全部用户")}</CardDescription></CardHeader><CardContent><TraderTable t={t} token={token} traders={traderList} onSelect={id => navigate(`/traders/${id}`)} onChanged={onChanged} showOwner /></CardContent></Card></div>
}

function Metric({ label, value }: { label: string; value: number }) {
  return <Card size="sm"><CardHeader><CardDescription>{label}</CardDescription><CardTitle>{value}</CardTitle></CardHeader></Card>
}
