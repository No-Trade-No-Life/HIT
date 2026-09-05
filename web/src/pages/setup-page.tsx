import { useMutation } from "@tanstack/react-query"
import { ShieldCheckIcon } from "lucide-react"
import { toast } from "sonner"

import { request } from "../lib/api"
import { showError } from "../lib/format"
import type { Copy } from "../lib/i18n"
import type { Me } from "../lib/types"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

export function SetupPage({ token, t, onDone }: { token: string; t: Copy; onDone: () => void }) {
  const mutation = useMutation({ mutationFn: () => request<Me>("/api/v1/setup", token, { method: "POST" }), onSuccess: () => { toast.success("Root user configured"); onDone() }, onError: showError })
  return <Card className="mx-auto mt-16 max-w-xl"><CardHeader><CardTitle>HIT initialization</CardTitle><CardDescription>The first authenticated user becomes the root user. This assigns HIT administration only; Auth Mini continues to own sign-in.</CardDescription></CardHeader><CardContent><Button onClick={() => mutation.mutate()} disabled={mutation.isPending}><ShieldCheckIcon data-icon="inline-start" />{t.setup}</Button></CardContent></Card>
}
