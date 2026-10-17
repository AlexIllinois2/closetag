/** The part of a repo's settings that can be edited and persisted. */
export interface RepoSettingsDraft {
  include_subfixs: string[]
  info_parse_fns: InfoParseFn[]
}

/**
 * Suffixes and info-parse rules are stored per repo. A repo carries
 * `using_global` when it has never saved rules of its own and is inheriting the
 * global defaults from config.toml.
 */
export interface RepoSettings extends RepoSettingsDraft {
  using_global: boolean
  global: RepoSettingsDraft
}

export interface RepoInfo {
  id: string
  name: string
  path: string
  exists: boolean
  item_count: number
  tag_count: number
  group_count: number
  db_size: number
  last_scan: string | null
  /** true when the repo inherits the global suffix/info-parse rules */
  using_global: boolean
}

export interface Tag {
  id: number
  name: string
  parent_id: number | null
  description: string
  item_count: number
  created_at: string
  updated_at: string
}

export interface TagBrief {
  id: number
  name: string
}

export interface Item {
  id: number
  /** parsed from the file name by the info-parse rules */
  name: string
  /** parsed from the file name by the info-parse rules (may be empty) */
  auth: string
  path: string
  tags: TagBrief[]
  created_at: string
  updated_at: string
}

export interface ItemsPage {
  total: number
  limit: number
  offset: number
  items: Item[]
}

export interface TagGroup {
  id: number
  name: string
  /** null = include every item */
  includes: number[] | null
  excludes: number[]
  description: string
  item_count: number
  created_at: string
  updated_at: string
}

export interface InfoParseFn {
  /** optional regex matched against the repo-relative path */
  match?: string
  /** delimiter used to split the file-name stem (extension stripped) into arr */
  split: string
  /** template rendered into `name`, e.g. "${arr[0]}" */
  name: string
  /** template rendered into `auth`, e.g. "${arr[1]}"; empty = no auth */
  auth: string
}

export interface ServerConfig {
  port: number
  open_browser: boolean
  exit_when_closed: boolean
}

export interface Config {
  repo_paths: string[]
  include_subfixs: string[]
  info_parse_fns: InfoParseFn[]
  server: ServerConfig
}

export interface ConfigResponse {
  config: Config
  data_dir: string
  version: string
  platform: string
}

export interface About {
  version: string
  data_dir: string
  platform: string
  arch: string
  port: number
}

export interface ScanReport {
  scanned: number
  added: number
  updated: number
  removed: number
  duration_ms: number
}

export interface InfoParsePreviewRule {
  index: number
  matched: boolean
  name: string | null
  auth: string | null
}

export interface InfoParsePreview {
  name: string
  auth: string
  rule_index: number | null
  used_fallback: boolean
  rules: InfoParsePreviewRule[]
}
