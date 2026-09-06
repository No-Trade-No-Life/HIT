import type { JsonSchema } from "../lib/types"
import { Skeleton } from "@/components/ui/skeleton"

export function SchemaValueList({ schema, value }: { schema?: JsonSchema; value: Record<string, unknown> }) {
  if (!schema) return <Skeleton className="h-48" />

  return <dl className="overflow-hidden rounded-md border">
    {Object.entries(schema.properties ?? {}).map(([name, field]) => <div className="grid gap-3 border-b p-3 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] sm:gap-6" key={name}>
      <dt className="flex flex-col gap-1">
        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1"><span className="font-medium">{field.title ?? name}</span><code className="text-xs text-muted-foreground">{name}</code></div>
        {field.description && <p className="m-0 text-sm text-muted-foreground">{field.description}</p>}
      </dt>
      <dd className="m-0 min-w-0 font-mono text-xs leading-5 break-words">{formatValue(value[name])}</dd>
    </div>)}
  </dl>
}

function formatValue(value: unknown) {
  if (value === undefined) return "—"
  if (typeof value === "string") return value
  return JSON.stringify(value)
}
