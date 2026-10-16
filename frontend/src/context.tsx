import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { api } from './api'
import type { Config, RepoInfo, RepoSettings } from './types'

export interface Toast {
  id: number
  kind: 'ok' | 'err'
  text: string
}

interface AppCtx {
  repos: RepoInfo[]
  currentRepoId: string | null
  currentRepo: RepoInfo | null
  setCurrentRepoId: (id: string) => void
  refreshRepos: () => Promise<void>
  config: Config | null
  refreshConfig: () => Promise<void>
  /** Suffix / id rules of the currently selected repo. */
  repoSettings: RepoSettings | null
  refreshRepoSettings: (repoId?: string) => Promise<void>
  toasts: Toast[]
  toast: (kind: 'ok' | 'err', text: string) => void
  dismissToast: (id: number) => void
}

const Ctx = createContext<AppCtx | null>(null)

export function AppProvider({ children }: { children: ReactNode }) {
  const [repos, setRepos] = useState<RepoInfo[]>([])
  const [currentRepoId, setCurrentRepoIdState] = useState<string | null>(
    () => localStorage.getItem('closetag.repo'),
  )
  const [config, setConfig] = useState<Config | null>(null)
  const [repoSettings, setRepoSettings] = useState<RepoSettings | null>(null)
  const [toasts, setToasts] = useState<Toast[]>([])

  const toast = useCallback((kind: 'ok' | 'err', text: string) => {
    const id = Date.now() + Math.random()
    setToasts((t) => [...t, { id, kind, text }])
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 4500)
  }, [])

  const dismissToast = useCallback((id: number) => {
    setToasts((t) => t.filter((x) => x.id !== id))
  }, [])

  const refreshRepos = useCallback(async () => {
    try {
      const list = await api.repos()
      setRepos(list)
      setCurrentRepoIdState((cur) => {
        if (list.length === 0) return null
        if (cur && list.some((r) => r.id === cur)) return cur
        return list[0].id
      })
    } catch (e) {
      toast('err', String(e))
    }
  }, [toast])

  const refreshConfig = useCallback(async () => {
    try {
      const res = await api.config()
      setConfig(res.config)
    } catch (e) {
      toast('err', String(e))
    }
  }, [toast])

  // Rules live per repo, so they follow the repo selector.
  const refreshRepoSettings = useCallback(
    async (repoId?: string) => {
      const id = repoId ?? currentRepoId
      if (!id) {
        setRepoSettings(null)
        return
      }
      try {
        setRepoSettings(await api.repoSettings(id))
      } catch (e) {
        setRepoSettings(null)
        toast('err', String(e))
      }
    },
    [currentRepoId, toast],
  )

  useEffect(() => {
    void refreshRepos()
    void refreshConfig()
  }, [refreshRepos, refreshConfig])

  useEffect(() => {
    void refreshRepoSettings()
  }, [refreshRepoSettings])

  const setCurrentRepoId = useCallback((id: string) => {
    setCurrentRepoIdState(id)
    localStorage.setItem('closetag.repo', id)
  }, [])

  const currentRepo = repos.find((r) => r.id === currentRepoId) ?? null

  return (
    <Ctx.Provider
      value={{
        repos,
        currentRepoId,
        currentRepo,
        setCurrentRepoId,
        refreshRepos,
        config,
        refreshConfig,
        repoSettings,
        refreshRepoSettings,
        toasts,
        toast,
        dismissToast,
      }}
    >
      {children}
    </Ctx.Provider>
  )
}

export function useApp(): AppCtx {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useApp must be used inside AppProvider')
  return ctx
}
