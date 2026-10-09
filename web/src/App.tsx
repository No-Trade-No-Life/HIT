import { useEffect, useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useAuthMini } from "auth-mini-react-components"
import { LinkitProvider, useLinkit } from "linkit-react-components"
import { AppLayout, type AppNavGroup } from "@zccz14/ux"
import { LayoutDashboardIcon, RefreshCwIcon, ShieldCheckIcon, WorkflowIcon, KeyRoundIcon, BotIcon, BookOpenIcon, HardDriveIcon } from "lucide-react"
import { Navigate, Route, Routes, useLocation } from "react-router-dom"

import { request, type AuthSdk } from "./lib/api"
import { copy, initialLocale, negotiateLocale, type Copy } from "./lib/i18n"
import type { LinkitStatus, Locale, Me } from "./lib/types"
import { HitMark } from "./components/hit-mark"
import { PageError } from "./components/trader-ui"
import { Button } from "@/components/ui/button"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import { AdminPage } from "./pages/admin-page"
import { CredentialsPage } from "./pages/credentials-page"
import { LinkitPage } from "./pages/linkit-page"
import { OverviewPage } from "./pages/overview-page"
import { SetupPage } from "./pages/setup-page"
import { TraderDetailPage } from "./pages/trader-detail-page"
import { TradersPage } from "./pages/traders-page"
import { TemplatesPage } from "./pages/templates-page"
import { SystemResourcesPage } from "./pages/system-resources-page"

export default function App() {
  const { isReady, isAuthenticated, sdk } = useAuthMini()
  const [locale, setLocale] = useState<Locale>(initialLocale)
  const t = copy[locale]
  if (!isReady || !isAuthenticated || !sdk) return <div className="grid min-h-svh place-items-center text-sm text-muted-foreground">{t.signin}</div>
  return <LinkitProvider linkitBaseUrl="https://linkit.ntnl.io" lang={locale}><HitShell auth={sdk} locale={locale} setLocale={setLocale} t={t} /></LinkitProvider>
}

function HitShell({ auth, locale, setLocale, t }: { auth: AuthSdk; locale: Locale; setLocale: (locale: Locale) => void; t: Copy }) {
  const { languages } = useLinkit()
  useEffect(() => {
    const next = negotiateLocale(languages)
    if (next) setLocale(next)
  }, [languages, setLocale])
  const queryClient = useQueryClient()
  const location = useLocation()
  const me = useQuery({ queryKey: ["me"], queryFn: () => request<Me>("/api/v1/me", auth) })
  const refresh = () => void queryClient.invalidateQueries()
  if (me.isPending) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>
  if (me.error) return <PageError error={me.error} />
  if (!me.data) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>
  if (me.data.setup_required && location.pathname !== "/setup") return <Navigate to="/setup" replace />
  if (!me.data.setup_required && location.pathname === "/setup") return <Navigate to="/" replace />

  const nav: AppNavGroup[] = [
    {
      label: locale === "zh" ? "工作台" : "Workspace",
      items: [
        { to: "/", label: t.overview, icon: <LayoutDashboardIcon /> },
        { to: "/traders", label: t.traders, icon: <WorkflowIcon /> },
        { to: "/templates", label: t.templates, icon: <BookOpenIcon /> },
        { to: "/credentials", label: t.credentials, icon: <KeyRoundIcon /> },
        { to: "/linkit", label: t.linkit, icon: <BotIcon /> },
      ],
    },
    ...(me.data.is_root
      ? [
          {
            label: locale === "zh" ? "系统" : "System",
            items: [
              { to: "/admin", label: t.admin, icon: <ShieldCheckIcon /> },
              { to: "/system-resources", label: t.systemResources, icon: <HardDriveIcon /> },
            ],
          },
        ]
      : []),
  ]

  return (
    <TooltipProvider>
      <LinkitAutoEnsure auth={auth} />
      <AppLayout
        logo={{ light: <HitMark className="size-7 shrink-0" />, dark: <HitMark className="size-7 shrink-0" /> }}
        title="HIT"
        nav={nav}
        pageTitle={pageTitle(location.pathname, t)}
        headerSlot={
          <Tooltip>
            <TooltipTrigger render={<Button variant="ghost" size="icon-sm" onClick={refresh} aria-label={t.refresh} />}>
              <RefreshCwIcon />
            </TooltipTrigger>
            <TooltipContent>{t.refresh}</TooltipContent>
          </Tooltip>
        }
      >
        <div className="mx-auto w-full max-w-7xl">
          <Routes>
            <Route path="/" element={<OverviewPage auth={auth} t={t} onChanged={refresh} />} />
            <Route path="/traders" element={<TradersPage auth={auth} t={t} onChanged={refresh} />} />
            <Route path="/traders/:traderId" element={<TraderDetailPage auth={auth} t={t} onChanged={refresh} />} />
            <Route path="/templates" element={<TemplatesPage t={t} />} />
            <Route path="/credentials" element={<CredentialsPage auth={auth} t={t} onChanged={refresh} />} />
            <Route path="/linkit" element={<LinkitPage auth={auth} t={t} />} />
            <Route path="/admin" element={me.data.is_root ? <AdminPage auth={auth} t={t} onChanged={refresh} /> : <Navigate to="/" replace />} />
            <Route path="/system-resources" element={me.data.is_root ? <SystemResourcesPage auth={auth} t={t} /> : <Navigate to="/" replace />} />
            <Route path="/setup" element={<SetupPage auth={auth} t={t} onDone={refresh} />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Routes>
        </div>
      </AppLayout>
    </TooltipProvider>
  )
}

// HIT keeps one Linkit Bot per user; this silent call provisions or repairs
// the connection on every workspace load, so notifications never need a setup
// step and the switch stays the only notification control.
function LinkitAutoEnsure({ auth }: { auth: AuthSdk }) {
  const client = useQueryClient()
  const ensure = useQuery({
    queryKey: ["linkit-ensure", auth.session.getState().sessionId],
    queryFn: () => request<LinkitStatus>("/api/v1/linkit", auth, { method: "POST" }),
    retry: false,
    staleTime: Infinity,
  })
  useEffect(() => {
    if (!ensure.isSuccess) return
    void client.invalidateQueries({ queryKey: ["linkit"] })
  }, [client, ensure.isSuccess])
  return null
}

function pageTitle(pathname: string, t: Copy) {
  if (pathname.startsWith("/traders")) return t.traders
  if (pathname === "/templates") return t.templates
  if (pathname === "/credentials") return t.credentials
  if (pathname === "/linkit") return t.linkit
  if (pathname === "/admin") return t.admin
  if (pathname === "/system-resources") return t.systemResources
  return t.overview
}
