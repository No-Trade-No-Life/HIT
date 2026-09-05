import { useQuery } from "@tanstack/react-query"
import { BookOpenIcon } from "lucide-react"

import { PageError } from "../components/trader-ui"
import type { Copy } from "../lib/i18n"
import type { JsonSchema, Template } from "../lib/types"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyTitle } from "@/components/ui/empty"
import { Skeleton } from "@/components/ui/skeleton"
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table"

export function TemplatesPage({ t }: { t: Copy }) {
  const templates = useQuery({ queryKey: ["templates"], queryFn: fetchTemplates })
  if (templates.isPending) return <Skeleton className="h-72" />
  if (templates.error) return <PageError error={templates.error} />
  const data = templates.data ?? []

  return <div className="flex flex-col gap-6">
    <div>
      <h1 className="m-0 text-2xl font-semibold tracking-tight">{t.templates}</h1>
      <p className="mt-1 max-w-3xl text-sm text-muted-foreground">{t.templatesDescription}</p>
    </div>
    {data.length ? <TemplateList templates={data} t={t} /> : <Empty>
      <EmptyHeader>
        <EmptyMedia variant="icon"><BookOpenIcon /></EmptyMedia>
        <EmptyTitle>{t.templates}</EmptyTitle>
        <EmptyDescription>{t.templatesDescription}</EmptyDescription>
      </EmptyHeader>
      <EmptyContent />
    </Empty>}
  </div>
}

async function fetchTemplates(): Promise<Template[]> {
  const response = await fetch("/api/templates")
  if (!response.ok) throw new Error("Failed to load strategy templates")
  return response.json() as Promise<Template[]>
}

function TemplateList({ templates, t }: { templates: Template[]; t: Copy }) {
  return <>
    <Card className="hidden md:block">
      <CardHeader>
        <CardTitle>{t.templates}</CardTitle>
        <CardDescription>{t.templatesDescription}</CardDescription>
      </CardHeader>
      <CardContent>
        <Table>
          <TableHeader><TableRow><TableHead>{t.template}</TableHead><TableHead>{t.credentialType}</TableHead><TableHead>{t.params}</TableHead><TableHead>{t.signalFields}</TableHead></TableRow></TableHeader>
          <TableBody>{templates.map(template => <TableRow key={template.id}>
            <TableCell className="min-w-72 align-top"><TemplateIdentity template={template} /></TableCell>
            <TableCell className="min-w-56 align-top"><CredentialType type={template.credential_type} /></TableCell>
            <TableCell className="min-w-72 align-top"><SchemaFields schema={template.params_schema} /></TableCell>
            <TableCell className="min-w-72 align-top"><SchemaFields schema={template.signal_schema} /></TableCell>
          </TableRow>)}</TableBody>
        </Table>
      </CardContent>
    </Card>
    <div className="flex flex-col gap-3 md:hidden">{templates.map(template => <Card key={template.id}>
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-3"><TemplateIdentity template={template} /><CredentialType type={template.credential_type} /></div>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <SchemaSection label={t.params} schema={template.params_schema} />
        <SchemaSection label={t.signalFields} schema={template.signal_schema} />
      </CardContent>
    </Card>)}</div>
  </>
}

function TemplateIdentity({ template }: { template: Template }) {
  return <div className="min-w-0">
    <p className="m-0 font-medium">{template.name}</p>
    <p className="mt-1 font-mono text-xs text-muted-foreground">{template.id}</p>
    <p className="mt-2 max-w-md text-sm text-muted-foreground">{template.description}</p>
  </div>
}

function CredentialType({ type }: { type: string }) {
  return <Badge variant="secondary"><code>{type}</code></Badge>
}

function SchemaSection({ label, schema }: { label: string; schema: JsonSchema }) {
  return <section className="flex flex-col gap-3"><h2 className="m-0 text-sm font-medium">{label}</h2><SchemaFields schema={schema} /></section>
}

function SchemaFields({ schema }: { schema: JsonSchema }) {
  return <div className="flex flex-col gap-3">{Object.entries(schema.properties ?? {}).map(([name, field]) => <div key={name}>
    <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1"><code>{name}</code><span className="text-sm font-medium">{field.title}</span></div>
    <p className="mt-1 text-sm text-muted-foreground">{field.description}</p>
  </div>)}</div>
}
