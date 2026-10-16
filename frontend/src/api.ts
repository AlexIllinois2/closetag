import type {
  About,
  Config,
  ConfigResponse,
  IdGenFn,
  IdGenPreview,
  Item,
  ItemsPage,
  RepoInfo,
  RepoSettings,
  RepoSettingsDraft,
  ScanReport,
  Tag,
  TagGroup,
} from './types'

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  })
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`
    try {
      const data = (await res.json()) as { error?: string }
      if (data?.error) msg = data.error
    } catch {
      /* keep default message */
    }
    throw new Error(msg)
  }
  return (await res.json()) as T
}

function post<T>(path: string, body?: unknown): Promise<T> {
  return req<T>(path, { method: 'POST', body: body === undefined ? undefined : JSON.stringify(body) })
}

function put<T>(path: string, body?: unknown): Promise<T> {
  return req<T>(path, { method: 'PUT', body: body === undefined ? undefined : JSON.stringify(body) })
}

function del<T>(path: string): Promise<T> {
  return req<T>(path, { method: 'DELETE' })
}

export interface ItemsQuery {
  tag?: number
  group?: number
  q?: string
  limit?: number
  offset?: number
}

function qs(params: Record<string, string | number | undefined>): string {
  const sp = new URLSearchParams()
  for (const [k, v] of Object.entries(params)) {
    if (v !== undefined && v !== '') sp.set(k, String(v))
  }
  const s = sp.toString()
  return s ? `?${s}` : ''
}

export const api = {
  repos: () => req<RepoInfo[]>('/repos'),
  addRepo: (path: string) => post<RepoInfo>('/repos', { path }),
  removeRepo: (id: string) => del<{ removed: string }>(`/repos/${encodeURIComponent(id)}`),
  scan: (repo: string) => post<ScanReport>(`/repos/${encodeURIComponent(repo)}/scan`),

  tags: (repo: string) => req<Tag[]>(`/repos/${encodeURIComponent(repo)}/tags`),
  createTag: (repo: string, body: { name: string; parent_id: number | null; description?: string }) =>
    post<Tag>(`/repos/${encodeURIComponent(repo)}/tags`, body),
  updateTag: (repo: string, id: number, body: { name?: string; parent_id?: number | null; description?: string }) =>
    put<Tag>(`/repos/${encodeURIComponent(repo)}/tags/${id}`, body),
  deleteTag: (repo: string, id: number) =>
    del<{ deleted: number }>(`/repos/${encodeURIComponent(repo)}/tags/${id}`),

  items: (repo: string, params: ItemsQuery = {}) =>
    req<ItemsPage>(`/repos/${encodeURIComponent(repo)}/items${qs({ ...params })}`),
  createItem: (repo: string, body: { path: string; tags?: number[] }) =>
    post<Item>(`/repos/${encodeURIComponent(repo)}/items`, body),
  updateItem: (repo: string, pk: number, body: { path?: string; tags?: number[] }) =>
    put<Item>(`/repos/${encodeURIComponent(repo)}/items/${pk}`, body),
  deleteItem: (repo: string, pk: number) =>
    del<{ deleted: number }>(`/repos/${encodeURIComponent(repo)}/items/${pk}`),
  addItemTags: (repo: string, pk: number, tagIds: number[]) =>
    post<Item>(`/repos/${encodeURIComponent(repo)}/items/${pk}/tags`, { tag_ids: tagIds }),
  removeItemTag: (repo: string, pk: number, tagId: number) =>
    del<{ deleted: unknown }>(`/repos/${encodeURIComponent(repo)}/items/${pk}/tags/${tagId}`),

  groups: (repo: string) => req<TagGroup[]>(`/repos/${encodeURIComponent(repo)}/groups`),
  createGroup: (repo: string, body: { name: string; includes: number[] | null; excludes?: number[]; description?: string }) =>
    post<TagGroup>(`/repos/${encodeURIComponent(repo)}/groups`, body),
  updateGroup: (
    repo: string,
    id: number,
    body: { name?: string; includes?: number[] | null; excludes?: number[]; description?: string },
  ) => put<TagGroup>(`/repos/${encodeURIComponent(repo)}/groups/${id}`, body),
  deleteGroup: (repo: string, id: number) =>
    del<{ deleted: number }>(`/repos/${encodeURIComponent(repo)}/groups/${id}`),
  groupItems: (repo: string, id: number, params: ItemsQuery = {}) =>
    req<ItemsPage>(`/repos/${encodeURIComponent(repo)}/groups/${id}/items${qs({ ...params })}`),

  repoSettings: (repo: string) =>
    req<RepoSettings>(`/repos/${encodeURIComponent(repo)}/settings`),
  saveRepoSettings: (repo: string, body: RepoSettingsDraft) =>
    put<RepoSettings>(`/repos/${encodeURIComponent(repo)}/settings`, body),
  /** Drop the repo's own rules so it inherits the global defaults again. */
  resetRepoSettings: (repo: string) =>
    del<RepoSettings>(`/repos/${encodeURIComponent(repo)}/settings`),

  config: () => req<ConfigResponse>('/config'),
  saveConfig: (cfg: Config) => put<{ ok: boolean }>('/config', cfg),
  about: () => req<About>('/about'),
  idgenPreview: (path: string, rules?: IdGenFn[]) =>
    post<IdGenPreview>('/idgen/preview', { path, rules }),
}
