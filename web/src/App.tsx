import { useState } from "react"
import { useQuery, useQueryClient } from "@tanstack/react-query"
import { useAuthMini } from "auth-mini-react-components"
import { LinkitAppHeaderUser, LinkitProvider } from "linkit-react-components"
import { LanguagesIcon, LayoutDashboardIcon, RefreshCwIcon, ShieldCheckIcon, WorkflowIcon, KeyRoundIcon, BotIcon, type LucideIcon } from "lucide-react"
import { Navigate, Route, Routes, useLocation, useNavigate } from "react-router-dom"

import { request } from "./lib/api"
import { copy, initialLocale, type Copy } from "./lib/i18n"
import type { Locale, Me } from "./lib/types"
import { PageError } from "./components/trader-ui"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectGroup, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Separator } from "@/components/ui/separator"
import { Sidebar, SidebarContent, SidebarGroup, SidebarGroupContent, SidebarGroupLabel, SidebarHeader, SidebarInset, SidebarMenu, SidebarMenuButton, SidebarMenuItem, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar"
import { Skeleton } from "@/components/ui/skeleton"
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip"
import { AdminPage } from "./pages/admin-page"
import { CredentialsPage } from "./pages/credentials-page"
import { LinkitPage } from "./pages/linkit-page"
import { OverviewPage } from "./pages/overview-page"
import { SetupPage } from "./pages/setup-page"
import { TraderDetailPage } from "./pages/trader-detail-page"
import { TradersPage } from "./pages/traders-page"

export default function App() {
  const { isReady, isAuthenticated, session } = useAuthMini()
  const [locale, setLocale] = useState<Locale>(initialLocale)
  const t = copy[locale]
  const token = session?.accessToken ?? ""
  if (!isReady || !isAuthenticated) return <div className="grid min-h-svh place-items-center text-sm text-muted-foreground">{t.signin}</div>
  return <LinkitProvider linkitBaseUrl="https://linkit.ntnl.io" lang={locale}><HitShell token={token} locale={locale} setLocale={setLocale} t={t} /></LinkitProvider>
}

function HitShell({ token, locale, setLocale, t }: { token: string; locale: Locale; setLocale: (locale: Locale) => void; t: Copy }) {
  const queryClient = useQueryClient()
  const location = useLocation()
  const navigate = useNavigate()
  const me = useQuery({ queryKey: ["me", token], queryFn: () => request<Me>("/api/v1/me", token) })
  const refresh = () => void queryClient.invalidateQueries()
  if (me.isPending) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>
  if (me.error) return <PageError error={me.error} />
  if (!me.data) return <div className="grid min-h-svh place-items-center"><Skeleton className="h-8 w-48" /></div>
  if (me.data.setup_required && location.pathname !== "/setup") return <Navigate to="/setup" replace />
  if (!me.data.setup_required && location.pathname === "/setup") return <Navigate to="/" replace />

  return <TooltipProvider><SidebarProvider><Sidebar collapsible="icon"><SidebarHeader className="px-3 py-4"><div className="flex items-center gap-2 font-semibold"><div className="grid size-7 place-items-center rounded-md bg-primary text-primary-foreground">H</div><span className="group-data-[collapsible=icon]:hidden">HIT</span></div></SidebarHeader><SidebarContent><SidebarGroup><SidebarGroupLabel>{locale === "zh" ? "工作台" : "Workspace"}</SidebarGroupLabel><SidebarGroupContent><SidebarMenu><NavItem icon={LayoutDashboardIcon} active={location.pathname === "/"} onClick={() => navigate("/")}>{t.overview}</NavItem><NavItem icon={WorkflowIcon} active={location.pathname.startsWith("/traders")} onClick={() => navigate("/traders")}>{t.traders}</NavItem><NavItem icon={KeyRoundIcon} active={location.pathname === "/credentials"} onClick={() => navigate("/credentials")}>{t.credentials}</NavItem><NavItem icon={BotIcon} active={location.pathname === "/linkit"} onClick={() => navigate("/linkit")}>{t.linkit}</NavItem></SidebarMenu></SidebarGroupContent></SidebarGroup>{me.data.is_root && <SidebarGroup><SidebarGroupLabel>{locale === "zh" ? "系统" : "System"}</SidebarGroupLabel><SidebarGroupContent><SidebarMenu><NavItem icon={ShieldCheckIcon} active={location.pathname === "/admin"} onClick={() => navigate("/admin")}>{t.admin}</NavItem></SidebarMenu></SidebarGroupContent></SidebarGroup>}</SidebarContent></Sidebar><SidebarInset><header className="sticky top-0 z-10 flex h-14 items-center gap-3 border-b bg-background/95 px-4 backdrop-blur supports-backdrop-filter:bg-background/85"><SidebarTrigger /><Separator orientation="vertical" className="h-5" /><div className="min-w-0 flex-1"><p className="truncate text-sm font-medium">{pageTitle(location.pathname, t)}</p></div>{me.data.is_root && <Badge variant="outline"><ShieldCheckIcon data-icon="inline-start" />{t.root}</Badge>}<Select value={locale} onValueChange={value => setLocale(value as Locale)}><SelectTrigger aria-label={t.language} size="sm"><LanguagesIcon /><SelectValue /></SelectTrigger><SelectContent><SelectGroup><SelectItem value="zh">中文</SelectItem><SelectItem value="en">English</SelectItem></SelectGroup></SelectContent></Select><Tooltip><TooltipTrigger render={<Button variant="ghost" size="icon-sm" onClick={refresh} aria-label={t.refresh} />}><RefreshCwIcon /></TooltipTrigger><TooltipContent>{t.refresh}</TooltipContent></Tooltip><LinkitAppHeaderUser lang={locale} /></header><main className="mx-auto w-full max-w-7xl p-4 md:p-6"><Routes><Route path="/" element={<OverviewPage token={token} t={t} onChanged={refresh} />} /><Route path="/traders" element={<TradersPage token={token} t={t} onChanged={refresh} />} /><Route path="/traders/:traderId" element={<TraderDetailPage token={token} t={t} onChanged={refresh} />} /><Route path="/credentials" element={<CredentialsPage token={token} t={t} onChanged={refresh} />} /><Route path="/linkit" element={<LinkitPage token={token} t={t} />} /><Route path="/admin" element={me.data.is_root ? <AdminPage token={token} t={t} onChanged={refresh} /> : <Navigate to="/" replace />} /><Route path="/setup" element={<SetupPage token={token} t={t} onDone={refresh} />} /><Route path="*" element={<Navigate to="/" replace />} /></Routes></main></SidebarInset></SidebarProvider></TooltipProvider>
}

function NavItem({ icon: Icon, active, onClick, children }: { icon: LucideIcon; active: boolean; onClick: () => void; children: string }) {
  return <SidebarMenuItem><SidebarMenuButton isActive={active} onClick={onClick}><Icon /><span>{children}</span></SidebarMenuButton></SidebarMenuItem>
}

function pageTitle(pathname: string, t: Copy) {
  if (pathname.startsWith("/traders")) return t.traders
  if (pathname === "/credentials") return t.credentials
  if (pathname === "/linkit") return t.linkit
  if (pathname === "/admin") return t.admin
  return t.overview
}
