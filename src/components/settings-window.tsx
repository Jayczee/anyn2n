import { useState, useEffect, useCallback } from "react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Shield, ShieldAlert, ShieldCheck } from "lucide-react"
import { toast } from "sonner"
import { n2nApi, type FirewallStatus } from "@/lib/n2n-api"

export default function SettingsWindow() {
  const [tab, setTab] = useState<"general" | "firewall">("general")
  const [fw, setFw] = useState<FirewallStatus | null>(null)
  const [applying, setApplying] = useState(false)
  const [closeBehavior, setCloseBehavior] = useState("minimize")

  useEffect(() => {
    n2nApi.getCloseBehavior().then(b => setCloseBehavior(b)).catch(() => {})
  }, [])

  const reload = useCallback(async () => {
    try { setFw(await n2nApi.checkFirewallStatus()) } catch { /* ignore */ }
  }, [])

  useEffect(() => { if (tab === "firewall") reload() }, [tab, reload])

  const handleCloseBehavior = async (b: string) => {
    setCloseBehavior(b)
    try { await n2nApi.setCloseBehavior(b) } catch { /* ignore */ }
  }

  const handleAddRule = async () => {
    setApplying(true)
    try { const msg = await n2nApi.addFirewallRule(); toast.success(msg); reload() }
    catch (e) { toast.error("添加规则失败", { description: String(e) }) }
    finally { setApplying(false) }
  }

  const handleDisable = async () => {
    setApplying(true)
    try { const msg = await n2nApi.disableFirewall(); toast.success(msg); reload() }
    catch (e) { toast.error("关闭防火墙失败", { description: String(e) }) }
    finally { setApplying(false) }
  }

  return (
    <div className="h-screen bg-background flex flex-col">
      <div className="p-3 border-b">
        <div className="flex gap-1">
          {(["general", "firewall"] as const).map(t => (
            <button
              key={t}
              className={`text-[10px] px-2 py-0.5 rounded border ${tab === t ? "bg-accent text-foreground border-border" : "text-muted-foreground border-transparent hover:border-border"}`}
              onClick={() => setTab(t)}
            >
              {t === "general" ? "通用" : "防火墙"}
            </button>
          ))}
        </div>
      </div>
      <div className="flex-1 overflow-auto p-3">
        {tab === "general" && (
          <div>
            <h2 className="text-xs font-semibold mb-2">关闭行为</h2>
            <p className="text-[10px] text-muted-foreground mb-2">点击右上角 × 关闭程序时：</p>
            <div className="space-y-1.5">
              <label className={`flex items-center gap-2 p-2 border rounded cursor-pointer text-xs ${closeBehavior === "close" ? "border-primary bg-accent" : ""}`}
                onClick={() => handleCloseBehavior("close")}>
                <div className={`size-3 rounded-full border-2 flex items-center justify-center ${closeBehavior === "close" ? "border-primary" : ""}`}>
                  {closeBehavior === "close" && <div className="size-1.5 rounded-full bg-primary" />}
                </div>
                直接关闭
                <span className="text-[10px] text-muted-foreground">（关闭前弹窗确认）</span>
              </label>
              <label className={`flex items-center gap-2 p-2 border rounded cursor-pointer text-xs ${closeBehavior === "minimize" ? "border-primary bg-accent" : ""}`}
                onClick={() => handleCloseBehavior("minimize")}>
                <div className={`size-3 rounded-full border-2 flex items-center justify-center ${closeBehavior === "minimize" ? "border-primary" : ""}`}>
                  {closeBehavior === "minimize" && <div className="size-1.5 rounded-full bg-primary" />}
                </div>
                最小化到托盘
              </label>
            </div>
          </div>
        )}
        {tab === "firewall" && (
          <div>
            <h2 className="text-xs font-semibold mb-2 flex items-center gap-1.5">
              <Shield className="size-3.5" />防火墙
            </h2>
            {!fw ? (
              <p className="text-xs text-muted-foreground">检测中...</p>
            ) : (
              <div className="space-y-2">
                <div className="flex items-center gap-2 text-xs">
                  <span className="text-muted-foreground">系统防火墙:</span>
                  {fw.enabled ? (
                    <Badge className="text-[10px] h-4 bg-yellow-600"><ShieldAlert className="size-2.5 mr-0.5" />已开启</Badge>
                  ) : (
                    <Badge className="text-[10px] h-4 bg-green-600"><ShieldCheck className="size-2.5 mr-0.5" />已关闭</Badge>
                  )}
                </div>
                <div className="flex items-center gap-2 text-xs">
                  <span className="text-muted-foreground">放行规则:</span>
                  {fw.rule_exists ? (
                    <Badge className="text-[10px] h-4 bg-green-600">已添加</Badge>
                  ) : (
                    <Badge className="text-[10px] h-4" variant="secondary">未添加</Badge>
                  )}
                </div>
                {fw.enabled && !fw.rule_exists && (
                  <div className="space-y-1.5">
                    <p className="text-[10px] text-muted-foreground">
                      按<strong>程序路径</strong>放行，移动程序位置后需重新点击。
                    </p>
                    <Button size="xs" onClick={handleAddRule} disabled={applying}>
                      <Shield className="size-3 mr-1" />
                      {applying ? "添加中..." : "一键放行 AnyN2N"}
                    </Button>
                  </div>
                )}
                {fw.enabled && (
                  <div className="pt-1 border-t">
                    <p className="text-[10px] text-muted-foreground mb-1">或者直接关闭系统防火墙（影响所有程序）：</p>
                    <Button size="xs" variant="destructive" onClick={handleDisable} disabled={applying}>
                      <ShieldAlert className="size-3 mr-1" />
                      {applying ? "关闭中..." : "关闭防火墙"}
                    </Button>
                  </div>
                )}
                {fw.enabled && fw.rule_exists && (
                  <p className="text-[10px] text-muted-foreground">AnyN2N Edge 已被防火墙放行，无需额外操作。</p>
                )}
                {!fw.enabled && (
                  <p className="text-[10px] text-muted-foreground">防火墙未开启，无需添加放行规则。</p>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
