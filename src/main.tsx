import { StrictMode, lazy, Suspense } from "react"
import { createRoot } from "react-dom/client"

import "./index.css"
import App from "./App.tsx"
import { ThemeProvider } from "@/components/theme-provider.tsx"

// 禁用右键菜单和刷新快捷键（所有窗口生效）
window.addEventListener("contextmenu", (e) => e.preventDefault())
window.addEventListener("keydown", (e) => {
  // F5 / Ctrl+R / Ctrl+Shift+R
  if (e.key === "F5" || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r")) {
    e.preventDefault()
  }
})

const PeersWindow = lazy(() => import("./components/peers-window.tsx"))
const LogsWindow = lazy(() => import("./components/logs-window.tsx"))
const ServersWindow = lazy(() => import("./components/servers-window.tsx"))
const SettingsWindow = lazy(() => import("./components/settings-window.tsx"))

function WindowRouter() {
  const params = new URLSearchParams(window.location.search)
  const view = params.get("view")

  const loading = (
    <main className="h-screen bg-background flex items-center justify-center">
      <p className="text-xs text-muted-foreground">加载中...</p>
    </main>
  )

  return (
    <Suspense fallback={loading}>
      {view === "peers" ? <PeersWindow /> :
       view === "logs" ? <LogsWindow /> :
       view === "servers" ? <ServersWindow /> :
       view === "settings" ? <SettingsWindow /> :
       <App />}
    </Suspense>
  )
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <WindowRouter />
    </ThemeProvider>
  </StrictMode>
)
