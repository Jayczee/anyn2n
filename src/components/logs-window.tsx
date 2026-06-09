import { useState, useEffect, useRef, useMemo } from "react"
import { Input } from "@/components/ui/input"
import { ScrollArea } from "@/components/ui/scroll-area"
import { Search } from "lucide-react"
import { n2nApi } from "@/lib/n2n-api"

export default function LogsWindow() {
  const [logs, setLogs] = useState<string[]>([])
  const [search, setSearch] = useState("")
  const bottomRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const t = setInterval(async () => {
      try { setLogs(await n2nApi.getLogs()) } catch { /* ignore */ }
    }, 500)
    return () => clearInterval(t)
  }, [])

  // 日志更新时自动滚动到底部
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "instant" })
  }, [logs])

  const filtered = useMemo(() => {
    if (!search.trim()) return logs
    const q = search.toLowerCase()
    return logs.filter((l) => l.toLowerCase().includes(q))
  }, [logs, search])

  const count = logs.length
  const shown = filtered.length

  return (
    <div className="h-screen bg-background flex flex-col">
      <div className="p-2 border-b space-y-2">
        <div className="flex items-center justify-between">
          <h1 className="text-sm font-bold">运行日志</h1>
          <span className="text-[10px] text-muted-foreground">
            {count} 条{search && shown !== count ? ` / 显示 ${shown}` : ""}
          </span>
        </div>
        <div className="relative">
          <Search className="size-3 absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" />
          <Input
            placeholder="搜索日志..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-6 text-[10px] pl-6"
          />
        </div>
      </div>
      <div className="flex-1 overflow-hidden p-2">
        <ScrollArea className="h-full">
          <div className="bg-muted rounded-md p-2 font-mono text-[10px] leading-relaxed">
            {filtered.length === 0 ? (
              <p className="text-muted-foreground">
                {search ? "无匹配结果" : logs.length === 0 ? "等待..." : "无日志"}
              </p>
            ) : (
              filtered.map((l, i) => {
                // 高亮搜索词
                if (search.trim()) {
                  const parts = l.split(new RegExp(`(${search.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")})`, "gi"))
                  return (
                    <div key={i} className="break-all">
                      {parts.map((p, j) =>
                        p.toLowerCase() === search.toLowerCase() ? (
                          <mark key={j} className="bg-yellow-300 text-black px-0.5 rounded-sm">{p}</mark>
                        ) : (
                          <span key={j}>{p}</span>
                        )
                      )}
                    </div>
                  )
                }
                return <div key={i} className="break-all">{l}</div>
              })
            )}
            <div ref={bottomRef} />
          </div>
        </ScrollArea>
      </div>
    </div>
  )
}
