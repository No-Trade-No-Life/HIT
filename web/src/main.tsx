import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { AuthMiniProvider } from "auth-mini-react-components"

import "./index.css"
import App from "./App"

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 5_000 } } })

createRoot(document.getElementById("root")!).render(
  <StrictMode><QueryClientProvider client={queryClient}><AuthMiniProvider authMiniBaseUrl="https://auth.ntnl.io" autoRedirectToLogin><App /></AuthMiniProvider></QueryClientProvider></StrictMode>,
)
