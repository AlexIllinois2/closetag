import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { api } from './api'
import Modal from './components/Modal'
import type { Config, RepoInfo, RepoSettings } from './types'

export interface Toast {
  id: number
  kind: 'ok' | 'err'
  text: string
}

interface Question {
  message: string
  confirmText: string
}

interface AppCtx {
  repos: RepoInfo[]
  currentRepoId: string | null
  currentRepo: RepoInfo | null
  setCurrentRepoId: (id: string) => void
  refreshRepos: () => Promise<void>
  config: Config | null
  refreshConfig: () => Promise<void>
  /** Suffix / info-parse rules of the currently selected repo. */
  repoSettings: RepoSettings | null
  refreshRepoSettings: (repoId?: string) => Promise<void>
  toasts: Toast[]
  toast: (kind: 'ok' | 'err', text: string) => void
  dismissToast: (id: number) => void
  /**
   * Ask before something destructive, answered by a dialog of the app itself.
   * The browser's `confirm` can be silenced by a page setting, and a silenced one
   * returns false: the button then looks dead with nothing said about why.
   */
  ask: (message: string, confirmText?: string) => Promise<boolean>
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
  const [question, setQuestion] = useState<Question | null>(null)
  const answered = useRef<((ok: boolean) => void) | null>(null)

  const ask = useCallback((message: string, confirmText = '删除') => {
    // A question left open is answered 取消 by the next one.
    answered.current?.(false)
    setQuestion({ message, confirmText })
    return new Promise<boolean>((resolve) => {
      answered.current = resolve
    })
  }, [])

  const answer = useCallback((ok: boolean) => {
    answered.current?.(ok)
    answered.current = null
    setQuestion(null)
  }, [])

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
        ask,
      }}
    >
      {children}
      {question && (
        <Modal
          title="确认"
          width={440}
          onClose={() => answer(false)}
          footer={
            <>
              <button className="btn" onClick={() => answer(false)}>
                取消
              </button>
              <button className="btn danger" onClick={() => answer(true)}>
                {question.confirmText}
              </button>
            </>
          }
        >
          <p className="muted">{question.message}</p>
        </Modal>
      )}
    </Ctx.Provider>
  )
}

export function useApp(): AppCtx {
  const ctx = useContext(Ctx)
  if (!ctx) throw new Error('useApp must be used inside AppProvider')
  return ctx
}
