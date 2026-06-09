import { useState, useEffect, useRef, useCallback } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { Badge } from "@/components/ui/badge"
import { ScrollText, Users, Plug, PlugZap, Server, ChevronDown, ArrowUp, ArrowDown, Settings } from "lucide-react"
import { toast } from "sonner"
import { invoke } from "@tauri-apps/api/core"
import { listen } from "@tauri-apps/api/event"
import { n2nApi, type EdgeStatus, type ServerEntry, type TapStats } from "@/lib/n2n-api"

interface ErrorResponse {
  error_type: string
  message: string
  suggestion: string
  detail: string
}

function parseError(e: unknown): ErrorResponse | null {
  if (typeof e === "string") {
    try { return JSON.parse(e) as ErrorResponse } catch { /* not JSON */ }
  }
  return null
}

function toastError(e: unknown) {
  const er = parseError(e)
  if (er) {
    toast.error(er.message, {
      description: er.suggestion,
      action: {
        label: "查看日志",
        onClick: () => n2nApi.openWindow("logs", "运行日志", 520, 500),
      },
    })
  } else {
    toast.error("发生错误", { description: String(e) })
  }
}

type ConnState = "idle" | "starting" | "sn_wait" | "connected" | "stopping"

export default function App() {
  const [serverAddress, setServerAddress] = useState("")
  const [customIp, setCustomIp] = useState("")
  const [autoGenerateIp, setAutoGenerateIp] = useState(true)
  const [communityName, setCommunityName] = useState("")
  const [connState, setConnState] = useState<ConnState>("idle")
  const [tunReady, setTunReady] = useState(false)
  const [snConnected, setSnConnected] = useState(false)
  const [edgeStatus, setEdgeStatus] = useState<EdgeStatus | null>(null)
  const connectingRef = useRef(false)
  const connectIdRef = useRef(0)  // 递增 ID，防止取消-重连竞态
  const autoIpRef = useRef(false) // 记录当前是否由自动分配获取了 IP
  const serverInputRef = useRef<HTMLInputElement>(null)
  const [savedServers, setSavedServers] = useState<ServerEntry[]>([])
  const [serverRttMap, setServerRttMap] = useState<Record<string, number | null>>({})
  const [serverDropdownOpen, setServerDropdownOpen] = useState(false)
  const zeroStats: TapStats = { rx_bytes: 0, tx_bytes: 0, rx_packets: 0, tx_packets: 0 }
  const [tapStats, setTapStats] = useState<TapStats>(zeroStats)
  const [statsHistory, setStatsHistory] = useState<{ txB: number; rxB: number; txP: number; rxP: number }[]>([])

  // 派生显示用的状态（必须在 callback 引用之前定义）
  const showConnecting = connState === "starting" || connState === "sn_wait"
  const isConnected = connState === "connected"
  const isStopping = connState === "stopping"
  const canConnect = !serverAddress || !communityName ? false : connState === "idle"
  const trayRef = useRef(false) // 上次托盘连接状态
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false)

  // 监听关闭确认事件 → 打开自定义弹窗
  useEffect(() => {
    const unlisten = listen("confirm-close", () => setConfirmCloseOpen(true))
    return () => { unlisten.then(f => f()) }
  }, [])

  // 连接状态下每秒轮询 TAP 网卡统计
  useEffect(() => {
    if (!isConnected) return
    const t = setInterval(async () => {
      try {
        const s = await n2nApi.getTapStats()
        // Rust 返回的是 sysinfo 差值（上次刷新以来的增量），直接用于速率显示
        setStatsHistory([{ txB: s.tx_bytes, rxB: s.rx_bytes, txP: s.tx_packets, rxP: s.rx_packets }])
        // 前端累加增量得到累计值
        setTapStats(prev => ({
          tx_bytes: (prev?.tx_bytes ?? 0) + Math.max(0, s.tx_bytes),
          rx_bytes: (prev?.rx_bytes ?? 0) + Math.max(0, s.rx_bytes),
          tx_packets: (prev?.tx_packets ?? 0) + Math.max(0, s.tx_packets),
          rx_packets: (prev?.rx_packets ?? 0) + Math.max(0, s.rx_packets),
        }))
      } catch { /* ignore */ }
    }, 1000)
    return () => { clearInterval(t); setStatsHistory([]); setTapStats(zeroStats) }
  }, [isConnected])

  const fmtBytes = (b: number) => {
    if (b >= 1_073_741_824) return `${(b / 1_073_741_824).toFixed(1)} GB`
    if (b >= 1_048_576) return `${(b / 1_048_576).toFixed(1)} MB`
    if (b >= 1024) return `${(b / 1024).toFixed(0)} KB`
    return `${b} B`
  }

  // 聚焦输入框时加载并展开下拉
  const handleServerInputFocus = useCallback(async () => {
    if (showConnecting || isConnected) return
    try {
      const list = await n2nApi.listServers()
      if (list.length === 0) return
      setSavedServers(list)
      const rtts: Record<string, number | null> = {}
      await Promise.all(
        list.map(async (s) => {
          try { rtts[s.id] = await n2nApi.measureServerRtt(s.ip, s.port) } catch { rtts[s.id] = null }
        })
      )
      setServerRttMap(rtts)
      setServerDropdownOpen(true)
    } catch { /* ignore */ }
  }, [showConnecting, isConnected])

  // 选中服务器
  const handleSelectServer = useCallback((s: ServerEntry) => {
    setServerAddress(`${s.ip}:${s.port}`)
    if (s.default_group) setCommunityName(s.default_group)
    setServerDropdownOpen(false)
  }, [])
  useEffect(() => {
    const t = setInterval(async () => {
      // 分别处理 status 和 logs，一个失败不影响另一个
      try {
        const r = await n2nApi.getStatus()
        setTunReady(r.tun_ready)
        setSnConnected(r.sn_connected)
        if (r.status) setEdgeStatus(r.status)

        setConnState(prev => {
          if (prev === "stopping") return "idle" // 断开完成
          if (!r.is_running) {
            // starting 是用户主动触发的，不因 is_running 短暂为 false 就跳回 idle
            return prev === "starting" ? "starting" : "idle"
          }
          if (r.sn_connected) return "connected"
          if (r.tun_ready) return "sn_wait"
          if (prev === "starting") return "starting"
          return "idle"
        })

        // 自动分配IP时，连接成功后回显获取到的IP
        if (autoIpRef.current && r.status?.local_ip && r.sn_connected) {
          setCustomIp(r.status.local_ip)
        }

        // 更新托盘图标
        const trayConnected = r.sn_connected
        if (trayRef.current !== trayConnected) {
          trayRef.current = trayConnected
          n2nApi.setTrayConnected(trayConnected).catch(() => {})
        }
      } catch { /* getStatus 失败，保持当前状态不变 */ }
    }, 800)
    return () => clearInterval(t)
  }, [])

  const handleConnect = useCallback(async () => {
    if (connectingRef.current) return
    connectingRef.current = true
    connectIdRef.current += 1
    const myId = connectIdRef.current
    autoIpRef.current = autoGenerateIp
    setConnState("starting")
    try {
      await n2nApi.connect({
        server_address: serverAddress,
        custom_ip: autoGenerateIp ? undefined : customIp || undefined,
        community_name: communityName,
      })
      // 如果在我等待期间用户取消了，静默忽略成功结果
      if (connectIdRef.current !== myId) return
    } catch (error) {
      if (connectIdRef.current !== myId) return
      toastError(error)
      setConnState("idle")
      autoIpRef.current = false
    } finally {
      if (connectIdRef.current === myId) {
        connectingRef.current = false
      }
    }
  }, [serverAddress, autoGenerateIp, customIp, communityName])

  const handleDisconnect = useCallback(async () => {
    connectingRef.current = false
    connectIdRef.current += 1 // 使任何进行中的 handleConnect 失效
    setConnState("stopping")
    setTunReady(false)
    setSnConnected(false)
    setEdgeStatus(null)
    if (autoIpRef.current) setCustomIp("")
    autoIpRef.current = false
    try { await n2nApi.disconnect() } catch { /* ignore */ }
    trayRef.current = false
    n2nApi.setTrayConnected(false).catch(() => {})
    setConnState("idle")
  }, [])


  return (
    <div className="h-screen bg-background p-3 flex flex-col gap-2">
      {/* 标题栏 */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {isConnected ? (
            <Badge className="text-[10px] h-4 px-1.5 bg-green-600">已连接</Badge>
          ) : connState === "starting" ? (
            <Badge className="text-[10px] h-4 px-1.5" variant="secondary">启动中</Badge>
          ) : connState === "sn_wait" ? (
            <Badge className="text-[10px] h-4 px-1.5 bg-yellow-600">等待 SN</Badge>
          ) : (
            <Badge className="text-[10px] h-4 px-1.5" variant="secondary">未连接</Badge>
          )}
          {tunReady && !snConnected && connState !== "starting" && (
            <Badge className="text-[10px] h-4 px-1.5 bg-blue-600">TUN 就绪</Badge>
          )}
        </div>
        <div className="flex items-center gap-1">
          {/* 服务器列表 */}
          <Button variant="ghost" size="icon-xs" title="服务器列表"
            onClick={() => n2nApi.openWindow("servers", "服务器列表", 520, 480)}>
            <Server className="size-3.5" />
          </Button>

          {/* 在线客户端 */}
          <Button variant="ghost" size="icon-xs" className="relative" title="在线客户端"
            onClick={() => n2nApi.openWindow("peers", "在线客户端", 380, 500)}>
            <Users className="size-3.5" />
            {(edgeStatus?.peer_count ?? 0) > 0 && (
              <span className="absolute -top-0.5 -right-0.5 bg-green-500 text-white text-[8px] rounded-full size-3 flex items-center justify-center">
                {edgeStatus?.peer_count}
              </span>
            )}
          </Button>

          {/* 设置 */}
          <Button variant="ghost" size="icon-xs" title="设置"
            onClick={() => n2nApi.openWindow("settings", "设置", 420, 400)}>
            <Settings className="size-3.5" />
          </Button>

          {/* 运行日志 */}
          <Button variant="ghost" size="icon-xs" title="运行日志"
            onClick={() => n2nApi.openWindow("logs", "运行日志", 520, 500)}>
            <ScrollText className="size-3.5" />
          </Button>
        </div>
      </div>

      {/* 配置区 */}
      <div className="flex flex-col gap-3">
        {/* 服务器地址 - 单独一行 */}
        <div className="w-full max-w-md mx-auto">
          <Label className="text-[10px]">Supernode 服务器</Label>
          <div className="relative">
            <Input
              ref={serverInputRef}
              placeholder=""
              value={serverAddress}
              onChange={e => setServerAddress(e.target.value)}
              onFocus={handleServerInputFocus}
              onBlur={() => setTimeout(() => setServerDropdownOpen(false), 200)}
              disabled={showConnecting || isConnected}
              className="h-8 text-sm text-center pr-7"
            />
            <button
              type="button"
              className="absolute right-1 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground p-0.5"
              onClick={() => {
                if (showConnecting || isConnected) return
                if (serverDropdownOpen) { setServerDropdownOpen(false); return }
                handleServerInputFocus()
              }}
              tabIndex={-1}
            >
              <ChevronDown className={`size-3.5 transition-transform ${serverDropdownOpen ? "rotate-180" : ""}`} />
            </button>

            {/* 服务器下拉列表 */}
            {serverDropdownOpen && savedServers.length > 0 && (
              <div className="absolute top-full left-0 right-0 mt-1 z-50 bg-popover border rounded-md shadow-md max-h-48 overflow-auto">
                {savedServers.map(s => (
                  <button
                    key={s.id}
                    type="button"
                    className="w-full text-left px-3 py-1.5 hover:bg-accent flex items-center justify-between gap-2 text-xs"
                    onMouseDown={e => { e.preventDefault(); handleSelectServer(s) }}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="font-medium truncate">{s.name || s.ip}</div>
                      <div className="text-[10px] text-muted-foreground">
                        {s.ip}:{s.port}
                        {s.default_group && <span className="ml-1.5">[{s.default_group}]</span>}
                      </div>
                    </div>
                    <span className={`font-mono text-[10px] shrink-0 ${
                      serverRttMap[s.id] === null || serverRttMap[s.id] === undefined
                        ? "text-muted-foreground"
                        : serverRttMap[s.id]! <= 80 ? "text-green-500" : serverRttMap[s.id]! <= 250 ? "text-yellow-500" : "text-red-500"
                    }`}>
                      {serverRttMap[s.id] !== null && serverRttMap[s.id] !== undefined ? `${serverRttMap[s.id]}ms` : "-"}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* 小组名称和IP地址 - 一行居中 */}
        <div className="w-full max-w-md mx-auto flex items-end gap-3">
          <div className="flex-1">
            <Label className="text-[10px]">小组名称</Label>
            <Input placeholder="" value={communityName} onChange={e => setCommunityName(e.target.value)}
              disabled={showConnecting || isConnected} className="h-7 text-xs text-center" />
          </div>
          <div className="flex-[1.4]">
            <div className="flex items-center justify-between mb-0.5">
              <Label className="text-[10px]">组内 IP 地址</Label>
              <div className="flex items-center gap-1">
                <Checkbox id="auto-ip" checked={autoGenerateIp} onCheckedChange={c => setAutoGenerateIp(c === true)}
                  disabled={showConnecting || isConnected} className="h-3 w-3" />
                <Label htmlFor="auto-ip" className="text-[10px] cursor-pointer whitespace-nowrap">自动</Label>
              </div>
            </div>
            <Input placeholder={autoGenerateIp ? "自动分配" : "请输入自定义IP"} value={customIp} onChange={e => setCustomIp(e.target.value)}
              disabled={showConnecting || isConnected || autoGenerateIp} className="h-7 text-xs text-center" />
          </div>
        </div>

        {/* 连接按钮 - 单独一行居中 */}
        <div className="w-full max-w-md mx-auto flex justify-center">
          {isConnected && !isStopping ? (
            <Button onClick={handleDisconnect} variant="destructive" size="xs">
              <PlugZap className="size-3 mr-1" />断开
            </Button>
          ) : isStopping ? (
            <Button disabled size="xs" variant="outline">断开中...</Button>
          ) : showConnecting ? (
            <Button onClick={handleDisconnect} variant="secondary" size="xs">
              <Plug className="size-3 mr-1 animate-pulse" />取消
            </Button>
          ) : (
            <Button onClick={handleConnect} disabled={!canConnect} size="xs">
              <Plug className="size-3 mr-1" />连接
            </Button>
          )}
        </div>
      </div>

      {/* 流量监控区 */}
      <div className="border-t pt-1.5 flex items-center gap-2 text-[10px]">
        <span className="flex items-center gap-0.5 text-green-600 shrink-0" title="总接收字节">
          <ArrowDown className="size-3" />{fmtBytes(tapStats.rx_bytes)}
        </span>
        <span className="flex items-center gap-0.5 text-blue-600 shrink-0" title="总发送字节">
          <ArrowUp className="size-3" />{fmtBytes(tapStats.tx_bytes)}
        </span>
        <span className="text-muted-foreground shrink-0" title="总接收包数 / 总发送包数">
           {tapStats.rx_packets.toLocaleString()}/{tapStats.tx_packets.toLocaleString()}
        </span>
        <span className="text-green-600 shrink-0 font-semibold" title="每秒接收字节数">
          {statsHistory.length > 0 ? fmtBytes(statsHistory[0].rxB) + "/s" : "0 B/s"}
        </span>
        <span className="text-blue-600 shrink-0 font-semibold" title="每秒发送字节数">
          {statsHistory.length > 0 ? fmtBytes(statsHistory[0].txB) + "/s" : "0 B/s"}
        </span>
      </div>

      {/* 关闭确认弹窗 */}
      {confirmCloseOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/40" onClick={() => setConfirmCloseOpen(false)} />
          <div className="relative bg-background rounded-lg border shadow-lg w-72 p-4">
            <h3 className="text-sm font-semibold mb-1">退出 AnyN2N</h3>
            <p className="text-xs text-muted-foreground mb-3">确定要退出程序吗？退出后连接将断开。</p>
            <div className="flex justify-end gap-2">
              <Button variant="outline" size="xs" onClick={() => setConfirmCloseOpen(false)}>取消</Button>
              <Button variant="destructive" size="xs" onClick={() => invoke("quit_app").catch(() => {})}>退出</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
