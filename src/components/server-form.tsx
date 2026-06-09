import { useState, useEffect } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { ServerEntry } from "@/lib/n2n-api"

interface Props {
  open: boolean
  onClose: () => void
  onSave: (entry: ServerEntry) => void
  edit?: ServerEntry | null
}

export function ServerForm({ open, onClose, onSave, edit }: Props) {
  const [name, setName] = useState("")
  const [ip, setIp] = useState("")
  const [port, setPort] = useState("")
  const [defaultGroup, setDefaultGroup] = useState("")
  const [error, setError] = useState("")

  useEffect(() => {
    if (open) {
      if (edit) {
        setName(edit.name)
        setIp(edit.ip)
        setPort(String(edit.port))
        setDefaultGroup(edit.default_group)
      } else {
        setName("")
        setIp("")
        setPort("")
        setDefaultGroup("")
      }
      setError("")
    }
  }, [open, edit])

  if (!open) return null

  const handleSave = () => {
    if (!ip.trim()) { setError("IP 地址不能为空"); return }
    if (!port.trim() || isNaN(Number(port))) { setError("端口号必须为数字"); return }
    const portNum = Number(port)
    if (portNum < 1 || portNum > 65535) { setError("端口号范围 1-65535"); return }

    onSave({
      id: edit?.id ?? "",
      name: name.trim() || ip.trim(),
      ip: ip.trim(),
      port: portNum,
      default_group: defaultGroup.trim(),
      created_at: edit?.created_at ?? 0,
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} />
      <div className="relative bg-background rounded-lg border shadow-lg w-[360px] p-4">
        <div className="mb-1">
          <h3 className="text-sm font-semibold">{edit ? "编辑服务器" : "添加服务器"}</h3>
        </div>
        <div className="space-y-3 mt-3">
          <div>
            <Label className="text-[10px]">名称 <span className="text-muted-foreground">(为空时用 IP 做名称)</span></Label>
            <Input value={name} onChange={e => setName(e.target.value)}
              placeholder="例如：公司 VPN" className="h-7 text-xs" />
          </div>
          <div>
            <Label className="text-[10px]">IP 地址 *</Label>
            <Input value={ip} onChange={e => setIp(e.target.value)}
              placeholder="1.2.3.4" className="h-7 text-xs" />
          </div>
          <div>
            <Label className="text-[10px]">端口号 *</Label>
            <Input value={port} onChange={e => setPort(e.target.value)}
              placeholder="52345" className="h-7 text-xs" />
          </div>
          <div>
            <Label className="text-[10px]">默认小组名称 <span className="text-muted-foreground">(可选)</span></Label>
            <Input value={defaultGroup} onChange={e => setDefaultGroup(e.target.value)}
              placeholder="group" className="h-7 text-xs" />
          </div>
          {error && <p className="text-[10px] text-destructive">{error}</p>}
          <div className="flex justify-end gap-2">
            <Button variant="outline" size="xs" onClick={onClose}>取消</Button>
            <Button size="xs" onClick={handleSave}>保存</Button>
          </div>
        </div>
      </div>
    </div>
  )
}
