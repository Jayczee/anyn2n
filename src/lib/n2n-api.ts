import { invoke } from "@tauri-apps/api/core"

export interface ConnectRequest {
  server_address: string
  custom_ip?: string
  community_name: string
  encryption_key?: string
}

export interface PeerInfo {
  mac_addr: string
  ip_addr: string
  connection_type: string // "p2p" | "relay"
  last_seen: number
}

export interface EdgeStatus {
  is_running: boolean
  uptime: number
  tx_packets: number
  rx_packets: number
  peers: PeerInfo[]
  supernode_addr: string
  peer_count: number
  forward_count: number
  last_super: number
  last_p2p: number
  community: string
  local_ip: string
  local_mac: string
  supernode_mac: string
}

export interface StatusResponse {
  is_running: boolean
  tun_ready: boolean
  sn_connected: boolean
  status?: EdgeStatus
}

export interface ServerEntry {
  id: string
  name: string
  ip: string
  port: number
  default_group: string
  created_at: number
}

export interface FirewallStatus {
  enabled: boolean
  rule_exists: boolean
  platform: string
}

export interface TapStats {
  rx_bytes: number
  tx_bytes: number
  rx_packets: number
  tx_packets: number
}

export const n2nApi = {
  getTapStats: () => invoke<TapStats>("get_tap_stats"),

  connect: (request: ConnectRequest) => invoke<string>("connect", { request }),
  disconnect: () => invoke<string>("disconnect"),
  getStatus: () => invoke<StatusResponse>("get_status"),
  getLogs: () => invoke<string[]>("get_logs"),

  // 服务器列表管理
  listServers: () => invoke<ServerEntry[]>("list_servers"),
  saveServer: (entry: ServerEntry) => invoke<ServerEntry>("save_server", { entry }),
  deleteServer: (id: string) => invoke<void>("delete_server", { id }),
  measureServerRtt: (ip: string, port: number) => invoke<number | null>("measure_server_rtt", { ip, port }),
  pingPeer: (ip: string) => invoke<number | null>("ping_peer", { ip }),
  checkFirewallStatus: () => invoke<FirewallStatus>("check_firewall_status"),
  addFirewallRule: () => invoke<string>("add_firewall_rule"),
  disableFirewall: () => invoke<string>("disable_firewall"),
  setCloseBehavior: (behavior: string) => invoke<void>("set_close_behavior", { behavior }),
  getCloseBehavior: () => invoke<string>("get_close_behavior_cmd"),
  setTrayConnected: (connected: boolean) => invoke<void>("set_tray_connected", { connected }),
  openWindow: (view: string, title: string, width: number, height: number) =>
    invoke<void>("open_window", { view, title, width, height }),
}
