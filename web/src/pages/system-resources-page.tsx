import { useQuery } from "@tanstack/react-query"

import { PageError } from "../components/trader-ui"
import { request } from "../lib/api"
import { formatBytes, formatTime } from "../lib/format"
import type { Copy } from "../lib/i18n"
import type { SystemResources } from "../lib/types"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Skeleton } from "@/components/ui/skeleton"

export function SystemResourcesPage({ token, t }: { token: string; t: Copy }) {
  const resources = useQuery({ queryKey: ["system-resources", token], queryFn: () => request<SystemResources>("/api/v1/system/resources", token), refetchInterval: 5_000 })
  if (resources.isPending) return <ResourceSkeleton />
  if (resources.error) return <PageError error={resources.error} />
  if (!resources.data) return <ResourceSkeleton />

  const value = resources.data
  return <div className="flex flex-col gap-6"><div><h1 className="m-0 text-2xl font-semibold tracking-tight">{t.systemResources}</h1><p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t.systemResourcesDescription}</p><p className="mt-2 text-xs text-muted-foreground">{t.updated}: {formatTime(value.sampled_at)}</p></div><div className="grid gap-4 sm:grid-cols-2"><ResourceCard title={t.cpu} description={t.cpuDescription} value={`${value.cpu.usage_percent.toFixed(1)}%`} details={[[t.cpuCores, String(value.cpu.logical_cpus)], [t.loadOneMinute, value.cpu.load_1m.toFixed(2)]]} /><ResourceCard title={t.memory} description={t.memoryDescription} value={`${formatBytes(value.memory.used_bytes)} / ${formatBytes(value.memory.total_bytes)}`} details={[[t.memoryUsed, formatBytes(value.memory.used_bytes)], [t.memoryAvailable, formatBytes(value.memory.available_bytes)]]} /><ResourceCard title={t.disk} description={t.diskDescription} value={value.disk ? `${formatBytes(value.disk.used_bytes)} / ${formatBytes(value.disk.total_bytes)}` : "—"} details={value.disk ? [[t.diskUsed, formatBytes(value.disk.used_bytes)], [t.diskAvailable, formatBytes(value.disk.available_bytes)], [t.mountPoint, value.disk.mount_point]] : []} /><ResourceCard title={t.sqlite} description={t.sqliteDescription} value={formatBytes(value.sqlite.total_bytes)} details={[[t.sqliteMain, formatBytes(value.sqlite.main_bytes)], [t.sqliteWal, formatBytes(value.sqlite.wal_bytes)], [t.sqliteShm, formatBytes(value.sqlite.shm_bytes)], [t.sqliteTotal, formatBytes(value.sqlite.total_bytes)]]} /></div></div>
}

function ResourceCard({ title, description, value, details }: { title: string; description: string; value: string; details: [string, string][] }) {
  return <Card><CardHeader><CardTitle>{title}</CardTitle><CardDescription>{description}</CardDescription></CardHeader><CardContent className="flex flex-col gap-4"><p className="m-0 font-mono text-2xl font-semibold tracking-tight">{value}</p><dl className="flex flex-col gap-2 text-sm">{details.map(([label, detail]) => <div className="flex items-baseline justify-between gap-4" key={label}><dt className="text-muted-foreground">{label}</dt><dd className="m-0 truncate font-mono text-xs">{detail}</dd></div>)}</dl></CardContent></Card>
}

function ResourceSkeleton() {
  return <div className="grid gap-4 sm:grid-cols-2">{Array.from({ length: 4 }, (_, index) => <Skeleton className="h-48" key={index} />)}</div>
}
