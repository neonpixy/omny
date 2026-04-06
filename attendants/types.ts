/** Types for Castle provider hooks — shapes returned by the daemon. */

/** Note metadata from idea.list */
export interface ManifestEntry {
  id: string
  root_id: string
  title: string
  creator: string
  extended_type: string
  path: string
  created: string
  modified: string
}

/** Full note package from idea.load */
export interface IdeaPackage {
  header: {
    id: string
    root_id: string
    author: string
    [key: string]: unknown
  }
  digits: Record<string, Digit>
}

/** Content block within an idea */
export interface Digit {
  id: string
  type: string
  content: unknown
  properties?: Record<string, unknown>
  author: string
  children?: string[]
}

/** Daemon config from config.get */
export interface DaemonConfig {
  omnibus: {
    port: number
    bind_all: boolean
    device_name: string
    enable_upnp: boolean
    home_node: string | null
    data_dir: string | null
  }
  tower: {
    enabled: boolean
    mode: string
    name: string
    seeds: string[]
    communities: string[]
    announce_interval_secs: number | null
    gospel_interval_secs: number | null
    gospel_live_interval_secs: number | null
    public_url: string | null
  }
}

/** Daemon status from daemon.status */
export interface DaemonStatus {
  running: boolean
  pid: number
  omnibus: {
    relay_port: number
    relay_connections: number
    pool_relays: number
    discovered_peers: number
    has_home_node: boolean
  }
  network?: {
    bind_all: boolean
    enable_upnp: boolean
    local_ip: string | null
    public_url: string | null
    relay_port: number
  }
  tower_enabled: boolean
  tower?: {
    mode: string
    name: string
    relay_connections: number
    gospel_peers: number
    event_count: number
    uptime_secs: number
  }
  crown: {
    exists: boolean
    unlocked: boolean
    crown_id?: string
  }
}
