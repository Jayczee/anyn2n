import { useState, useEffect, useCallback } from "react"
import { Button } from "@/components/ui/button"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Plus, Pencil, Trash2 } from "lucide-react"
import { toast } from "sonner"
import { n2nApi, type ServerEntry } from "@/lib/n2n-api"
import { ServerForm } from "@/components/server-form"

function rttColor(ms: number | null): string {
  if (ms === null || ms === undefined) return "text-muted-foreground"
  if (ms <= 80) return "text-green-500"
  if (ms <= 250) return "text-yellow-500"
  return "text-red-500"
}

export default function ServersWindow() {
  const [servers, setServers] = useState<ServerEntry[]>([])
  const [rttMap, setRttMap] = useState<Record<string, number | null>>({})
  const [formOpen, setFormOpen] = useState(false)
  const [editing, setEditing] = useState<ServerEntry | null>(null)
  const [measuring, setMeasuring] = useState(false)

  const reload = useCallback(async () => {
    try {
      const list = await n2nApi.listServers()
      setServers(list)
      setMeasuring(true)
      const rtts: Record<string, number | null> = {}
      await Promise.all(
        list.map(async (s) => {
          try { rtts[s.id] = await n2nApi.measureServerRtt(s.ip, s.port) } catch { rtts[s.id] = null }
        })
      )
      setRttMap(rtts)
      setMeasuring(false)
    } catch { /* ignore */ }
  }, [])

  useEffect(() => { reload() }, [reload])

  const handleSave = async (entry: ServerEntry) => {
    try {
      await n2nApi.saveServer(entry)
      setFormOpen(false)
      setEditing(null)
      reload()
    } catch (e) { toast.error("保存失败", { description: String(e) }) }
  }

  const handleDelete = async (id: string) => {
    try { await n2nApi.deleteServer(id); reload() } catch (e) { toast.error("删除失败", { description: String(e) }) }
  }

  return (
    <div className="h-screen bg-background flex flex-col">
      <div className="p-3 border-b flex items-center justify-between">
        <h1 className="text-sm font-bold">服务器列表</h1>
        <Button variant="outline" size="icon-xs" title="添加" onClick={() => { setEditing(null); setFormOpen(true) }}>
          <Plus className="size-3.5" />
        </Button>
      </div>
      <div className="flex-1 overflow-hidden p-3">
        {servers.length === 0 ? (
          <p className="text-xs text-muted-foreground text-center py-8">暂无服务器，点击 + 添加</p>
        ) : (
          <ScrollArea className="h-full">
            <table className="w-full text-xs">
              <thead>
                <tr className="border-b text-[10px] text-muted-foreground sticky top-0 bg-background">
                  <th className="text-left py-1.5 font-medium">名称</th>
                  <th className="text-left py-1.5 font-medium">IP</th>
                  <th className="text-left py-1.5 font-medium">端口</th>
                  <th className="text-left py-1.5 font-medium">延迟</th>
                  <th className="text-right py-1.5 font-medium w-16">操作</th>
                </tr>
              </thead>
              <tbody>
                {servers.map((s) => (
                  <tr key={s.id} className="border-b last:border-0 hover:bg-muted/50">
                    <td className="py-1.5 pr-2 font-medium truncate max-w-[100px]">
                      {s.name || `${s.ip}:${s.port}`}
                      {s.default_group && <span className="text-[10px] text-muted-foreground block">{s.default_group}</span>}
                    </td>
                    <td className="py-1.5 pr-2 font-mono text-[11px]">{s.ip}</td>
                    <td className="py-1.5 pr-2 font-mono text-[11px]">{s.port}</td>
                    <td className={`py-1.5 pr-2 font-mono text-[11px] ${rttColor(rttMap[s.id])}`}>
                      {measuring && rttMap[s.id] === undefined ? "..." : rttMap[s.id] !== null && rttMap[s.id] !== undefined ? `${rttMap[s.id]}ms` : "-"}
                    </td>
                    <td className="py-1.5 text-right">
                      <div className="flex items-center justify-end gap-0.5">
                        <Button variant="ghost" size="icon-xs" className="size-5" title="编辑" onClick={() => { setEditing(s); setFormOpen(true) }}>
                          <Pencil className="size-3" />
                        </Button>
                        <Button variant="ghost" size="icon-xs" className="size-5 text-destructive" title="删除" onClick={() => handleDelete(s.id)}>
                          <Trash2 className="size-3" />
                        </Button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </ScrollArea>
        )}
      </div>
      <ServerForm open={formOpen} onClose={() => { setFormOpen(false); setEditing(null) }} onSave={handleSave} edit={editing} />
    </div>
  )
}
