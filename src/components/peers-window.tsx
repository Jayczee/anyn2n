import { useState, useEffect, useCallback } from "react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import { RefreshCw } from "lucide-react"
import { n2nApi, type PeerInfo } from "@/lib/n2n-api"

function rttColor(ms: number | null): string {
  if (ms === null || ms === undefined) return "text-muted-foreground"
  if (ms <= 80) return "text-green-500"
  if (ms <= 250) return "text-yellow-500"
  return "text-red-500"
}

export default function PeersWindow() {
  const [peers, setPeers] = useState<PeerInfo[]>([])
  const [rttMap, setRttMap] = useState<Record<number, number | null>>({})
  const [connected, setConnected] = useState(false)
  const [refreshing, setRefreshing] = useState(false)

  // 拉取对等节点列表 + 测 RTT
  const fetch = useCallback(async () => {
    try {
      const r = await n2nApi.getStatus()
      setConnected(r.sn_connected)
      if (r.status) {
        setPeers(r.status.peers)
        // 并发 ping 每个 peer 的 VPN IP
        const rtts: Record<number, number | null> = {}
        await Promise.all(
          r.status.peers.map(async (_, i) => {
            try { rtts[i] = await n2nApi.pingPeer(r.status!.peers[i].ip_addr) } catch { rtts[i] = null }
          })
        )
        setRttMap(rtts)
      }
    } catch { /* ignore */ }
  }, [])

  // 定时轮询（不测 RTT，只在手动刷新时测）
  useEffect(() => {
    fetch()
    const t = setInterval(fetch, 3000)
    return () => clearInterval(t)
  }, [fetch])

  // 手动刷新（含 RTT 测量）
  const handleRefresh = async () => {
    setRefreshing(true)
    await fetch()
    setRefreshing(false)
  }

  return (
    <div className="h-screen bg-background flex flex-col">
      <div className="p-2 border-b flex items-center justify-between">
        <h1 className="text-sm font-bold">在线客户端</h1>
        <Button variant="ghost" size="icon-xs" title="刷新" onClick={handleRefresh} disabled={refreshing}>
          <RefreshCw className={`size-3.5 ${refreshing ? "animate-spin" : ""}`} />
        </Button>
      </div>
      <div className="flex-1 overflow-hidden p-2">
        {!connected && <p className="text-xs text-muted-foreground text-center py-8">未连接</p>}
        {connected && peers.length === 0 && <p className="text-xs text-muted-foreground text-center py-8">暂无其他客户端</p>}
        {connected && peers.length > 0 && (
          <ScrollArea className="h-full">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b text-[10px] text-muted-foreground sticky top-0 bg-background">
                  <th className="text-left py-1.5 font-medium">IP 地址</th>
                  <th className="text-left py-1.5 font-medium">MAC 地址</th>
                  <th className="text-left py-1.5 font-medium">连接方式</th>
                  <th className="text-right py-1.5 font-medium">RTT 延迟</th>
                </tr>
              </thead>
              <tbody>
                {peers.map((p, i) => (
                  <tr key={i} className="border-b last:border-0 hover:bg-muted/50">
                    <td className="py-1.5 pr-2 font-mono text-[11px]">{p.ip_addr}</td>
                    <td className="py-1.5 pr-2 font-mono text-[11px]">{p.mac_addr}</td>
                    <td className="py-1.5 pr-2">
                      <Badge className={`text-[10px] h-4 px-1 ${p.connection_type === "p2p" ? "bg-green-600" : ""}`}>
                        {p.connection_type === "p2p" ? "P2P" : "中转"}
                      </Badge>
                    </td>
                    <td className={`py-1.5 text-right font-mono text-[11px] ${rttColor(rttMap[i])}`}>
                      {rttMap[i] !== null && rttMap[i] !== undefined ? `${rttMap[i]}ms` : "-"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </ScrollArea>
        )}
      </div>
    </div>
  )
}
