import { useEffect, useState } from 'react'
import { api } from '../api'
import { useApp } from '../context'
import type { About, Config, IdGenFn, IdGenPreview, RepoSettingsDraft } from '../types'

/** Lowercase and ensure a leading dot so the stored form is comparable. */
function normSuffix(s: string): string {
  const t = s.trim().toLowerCase()
  if (!t) return ''
  return t.startsWith('.') ? t : `.${t}`
}

function SuffixEditor({
  suffixes,
  onChange,
}: {
  suffixes: string[]
  onChange: (next: string[]) => void
}) {
  const [input, setInput] = useState('')

  const add = () => {
    const norm = normSuffix(input)
    if (!norm) return
    if (suffixes.includes(norm)) {
      setInput('')
      return
    }
    onChange([...suffixes, norm])
    setInput('')
  }

  return (
    <>
      <div className="chips">
        {suffixes.map((s, i) => (
          <span key={`${s}-${i}`} className="chip">
            {s}
            <button
              title="移除"
              onClick={() => onChange(suffixes.filter((_, j) => j !== i))}
            >
              ✕
            </button>
          </span>
        ))}
        {suffixes.length === 0 && <span className="muted small">为空 = 扫描所有文件</span>}
      </div>
      <div className="row">
        <input
          className="input"
          placeholder=".rs"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              add()
              e.preventDefault()
            }
          }}
        />
        <button className="btn" onClick={add} disabled={!input.trim()}>
          添加
        </button>
      </div>
    </>
  )
}

function IdRulesEditor({
  rules,
  onChange,
}: {
  rules: IdGenFn[]
  onChange: (next: IdGenFn[]) => void
}) {
  const update = (i: number, patch: Partial<IdGenFn>) =>
    onChange(rules.map((r, j) => (j === i ? { ...r, ...patch } : r)))

  return (
    <>
      <p className="muted small">
        规则按顺序生效：<code>match</code> 正则命中相对路径后，用 <code>split</code> 把路径切分成{' '}
        <code>arr</code>，再渲染 <code>template</code>。模板可用 <code>{'${arr[0]||arr[1]}'}</code>
        （取第一个非空）、字符串拼接与括号；渲染为空则回退为完整相对路径。
      </p>
      {rules.map((rule, i) => (
        <div key={i} className="rule-row">
          <input
            className="input"
            placeholder="match 正则（可空）"
            value={rule.match ?? ''}
            onChange={(e) => update(i, { match: e.target.value || undefined })}
          />
          <input
            className="input narrow"
            placeholder="split"
            value={rule.split}
            onChange={(e) => update(i, { split: e.target.value })}
          />
          <input
            className="input"
            placeholder="${arr[0]||arr[1]}"
            value={rule.template}
            onChange={(e) => update(i, { template: e.target.value })}
          />
          <button
            className="btn small danger"
            title="删除规则"
            onClick={() => onChange(rules.filter((_, j) => j !== i))}
          >
            ✕
          </button>
        </div>
      ))}
      <button
        className="btn"
        onClick={() => onChange([...rules, { split: '-', template: '${arr[0]||arr[1]}' }])}
      >
        ＋ 添加规则
      </button>
    </>
  )
}

export default function ConfigPage() {
  const {
    config,
    refreshConfig,
    refreshRepos,
    repos,
    currentRepo,
    currentRepoId,
    repoSettings,
    refreshRepoSettings,
    toast,
  } = useApp()
  const [globalDraft, setGlobalDraft] = useState<Config | null>(null)
  const [repoDraft, setRepoDraft] = useState<RepoSettingsDraft | null>(null)
  const [about, setAbout] = useState<About | null>(null)
  const [newRepoPath, setNewRepoPath] = useState('')
  const [addingRepo, setAddingRepo] = useState(false)
  const [savingGlobal, setSavingGlobal] = useState(false)
  const [savingRepo, setSavingRepo] = useState(false)
  const [previewPath, setPreviewPath] = useState('src/utils/date-helper.ts')
  const [preview, setPreview] = useState<IdGenPreview | null>(null)

  useEffect(() => {
    if (config) setGlobalDraft(JSON.parse(JSON.stringify(config)) as Config)
  }, [config])

  // Repo rules are stored per repo, so reload them whenever the selection moves.
  useEffect(() => {
    if (repoSettings) {
      setRepoDraft({
        include_subfixs: [...repoSettings.include_subfixs],
        id_gen_fns: [...repoSettings.id_gen_fns],
      })
    } else {
      setRepoDraft(null)
    }
  }, [repoSettings])

  useEffect(() => {
    void api
      .about()
      .then(setAbout)
      .catch(() => {})
  }, [])

  // debounce the id-gen preview against the repo rules being edited
  useEffect(() => {
    if (!repoDraft || !previewPath.trim()) {
      setPreview(null)
      return
    }
    const timer = setTimeout(() => {
      api
        .idgenPreview(previewPath.trim(), repoDraft.id_gen_fns)
        .then(setPreview)
        .catch(() => setPreview(null))
    }, 300)
    return () => clearTimeout(timer)
  }, [previewPath, repoDraft])

  const saveGlobal = async () => {
    if (!globalDraft) return
    setSavingGlobal(true)
    try {
      await api.saveConfig(globalDraft)
      toast('ok', '全局默认已保存（只影响之后新建的仓库）')
      await refreshConfig()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setSavingGlobal(false)
    }
  }

  const saveRepoSettings = async () => {
    if (!repoDraft || !currentRepoId) return
    setSavingRepo(true)
    try {
      const saved = await api.saveRepoSettings(currentRepoId, repoDraft)
      setRepoDraft({
        include_subfixs: [...saved.include_subfixs],
        id_gen_fns: [...saved.id_gen_fns],
      })
      toast('ok', '仓库规则已保存')
      await refreshRepoSettings()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setSavingRepo(false)
    }
  }

  const resetRepoSettings = async () => {
    if (!currentRepoId) return
    if (!confirm('放弃该仓库的自定义规则，改为继承全局默认？')) return
    setSavingRepo(true)
    try {
      const saved = await api.resetRepoSettings(currentRepoId)
      setRepoDraft({
        include_subfixs: [...saved.include_subfixs],
        id_gen_fns: [...saved.id_gen_fns],
      })
      toast('ok', '已恢复继承全局默认')
      await refreshRepoSettings()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setSavingRepo(false)
    }
  }

  const applyGlobalToRepo = () => {
    if (!globalDraft) return
    setRepoDraft({
      include_subfixs: [...globalDraft.include_subfixs],
      id_gen_fns: [...globalDraft.id_gen_fns],
    })
    toast('ok', '已套用全局默认，记得保存')
  }

  const addRepo = async () => {
    const p = newRepoPath.trim()
    if (!p) return
    setAddingRepo(true)
    try {
      await api.addRepo(p)
      setNewRepoPath('')
      toast('ok', '仓库已添加（已继承当前全局默认规则）')
      await refreshRepos()
      await refreshConfig()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setAddingRepo(false)
    }
  }

  const removeRepo = async (path: string) => {
    const info = repos.find((r) => r.path === path)
    if (!info) return
    if (
      !confirm(
        `移除仓库「${info.path}」？\n该仓库的数据库（${info.item_count} 条目 / ${info.tag_count} 标签）会被删除，此操作不可恢复。`,
      )
    )
      return
    try {
      await api.removeRepo(info.id)
      toast('ok', '已移除')
      await refreshRepos()
      await refreshConfig()
    } catch (e) {
      toast('err', String(e))
    }
  }

  if (!globalDraft) {
    return (
      <div className="page">
        <div className="empty">
          <p>加载中…</p>
        </div>
      </div>
    )
  }

  return (
    <div className="page config-page">
      <div className="toolbar">
        <h2 className="page-title">配置</h2>
      </div>

      <section className="card">
        <h3>存储</h3>
        <div className="kv">
          <span className="muted">数据目录</span>
          <code>{about?.data_dir ?? ''}</code>
        </div>
        <div className="kv">
          <span className="muted">配置文件</span>
          <code>config.toml</code>
        </div>
        <div className="kv">
          <span className="muted">版本</span>
          <span>
            v{about?.version ?? '?'}（{about?.platform ?? '?'}/{about?.arch ?? '?'}）
          </span>
        </div>
        <p className="muted small">每个仓库一个独立的 SQLite 数据库，存放在数据目录的 repos/ 下。</p>
      </section>

      <section className="card">
        <h3>仓库（repo_paths）</h3>
        <div className="repo-list">
          {globalDraft.repo_paths.length === 0 && <p className="muted small">尚未添加仓库。</p>}
          {globalDraft.repo_paths.map((p) => {
            const info = repos.find((r) => r.path === p)
            return (
              <div key={p} className="repo-row">
                <span className={`dot ${info ? (info.exists ? 'dot-ok' : 'dot-bad') : 'dot-unknown'}`} />
                <code className="repo-path">{p}</code>
                <span className="muted small">
                  {info ? `${info.item_count} 条目 · ${info.tag_count} 标签` : '未加载'}
                  {info?.using_global ? ' · 继承全局规则' : ' · 自定义规则'}
                </span>
                <button className="btn small danger" onClick={() => void removeRepo(p)}>
                  移除
                </button>
              </div>
            )
          })}
        </div>
        <div className="row">
          <input
            className="input"
            placeholder="D:/path/to/repo"
            value={newRepoPath}
            onChange={(e) => setNewRepoPath(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void addRepo()
            }}
          />
          <button className="btn" onClick={() => void addRepo()} disabled={addingRepo || !newRepoPath.trim()}>
            {addingRepo ? '添加中…' : '添加仓库'}
          </button>
        </div>
        <p className="muted small">
          添加后会立即创建该仓库的数据库；移除会同时删除数据库文件（不可恢复）。
        </p>
      </section>

      <section className="card">
        <h3>当前仓库规则</h3>
        {!currentRepo ? (
          <p className="muted small">请先在页面顶部的仓库下拉框中选择一个仓库。</p>
        ) : (
          <>
            <div className="kv">
              <span className="muted">作用于</span>
              <code>{currentRepo.path}</code>
            </div>
            <p className="muted small">
              后缀与 ID 规则每个仓库各存一份（保存在该仓库自己的数据库里）。扫描与「新建/编辑条目」都只使用这里的规则。
              {repoSettings?.using_global
                ? ' 该仓库尚未自定义，当前正继承下方的全局默认。'
                : ' 该仓库已自定义，修改全局默认不会影响它。'}
            </p>

            <h4>文件后缀</h4>
            <SuffixEditor
              suffixes={repoDraft?.include_subfixs ?? []}
              onChange={(next) =>
                setRepoDraft((d) => (d ? { ...d, include_subfixs: next } : d))
              }
            />

            <h4>ID 生成规则</h4>
            <IdRulesEditor
              rules={repoDraft?.id_gen_fns ?? []}
              onChange={(next) => setRepoDraft((d) => (d ? { ...d, id_gen_fns: next } : d))}
            />

            <div className="preview-box">
              <label className="field">
                <span>预览路径</span>
                <input
                  className="input"
                  value={previewPath}
                  onChange={(e) => setPreviewPath(e.target.value)}
                  placeholder="src/utils/date-helper.ts"
                />
              </label>
              {preview && (
                <div className="preview-result">
                  <div>
                    生成 ID：<code>{preview.id}</code>{' '}
                    {preview.used_fallback ? (
                      <span className="muted small">（无规则命中，回退为完整路径）</span>
                    ) : (
                      <span className="muted small">（命中规则 #{preview.rule_index! + 1}）</span>
                    )}
                  </div>
                  <div className="muted small">
                    {preview.rules.map((r) => (
                      <div key={r.index}>
                        规则 {r.index + 1}：
                        {r.matched ? (r.id ? <code>{r.id}</code> : '模板为空') : '未命中'}
                      </div>
                    ))}
                  </div>
                </div>
              )}
            </div>

            <div className="row">
              <button className="btn primary" onClick={() => void saveRepoSettings()} disabled={savingRepo}>
                {savingRepo ? '保存中…' : '保存仓库规则'}
              </button>
              <button className="btn" onClick={applyGlobalToRepo}>
                套用全局默认
              </button>
              <button
                className="btn small danger"
                onClick={() => void resetRepoSettings()}
                disabled={savingRepo || repoSettings?.using_global}
              >
                恢复继承全局默认
              </button>
            </div>
          </>
        )}
      </section>

      <section className="card">
        <h3>全局默认（新仓库模板）</h3>
        <p className="muted small">
          下面两项是全局默认值：仓库没有自定义规则时使用它们；已经保存过自己规则的仓库不受影响。
          要改某个具体仓库，请在上方「当前仓库规则」里编辑。
        </p>

        <h4>文件后缀</h4>
        <SuffixEditor
          suffixes={globalDraft.include_subfixs}
          onChange={(next) =>
            setGlobalDraft({ ...globalDraft, include_subfixs: next })
          }
        />

        <h4>ID 生成规则</h4>
        <IdRulesEditor
          rules={globalDraft.id_gen_fns}
          onChange={(next) => setGlobalDraft({ ...globalDraft, id_gen_fns: next })}
        />

        <div className="row">
          <button className="btn primary" onClick={() => void saveGlobal()} disabled={savingGlobal}>
            {savingGlobal ? '保存中…' : '保存全局默认'}
          </button>
        </div>
      </section>

      <section className="card">
        <h3>服务（server）</h3>
        <div className="row wrap">
          <label className="field inline">
            <span>端口</span>
            <input
              className="input narrow"
              type="number"
              min={0}
              value={globalDraft.server.port}
              onChange={(e) =>
                setGlobalDraft({
                  ...globalDraft,
                  server: { ...globalDraft.server, port: Number(e.target.value) || 0 },
                })
              }
            />
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={globalDraft.server.open_browser}
              onChange={(e) =>
                setGlobalDraft({
                  ...globalDraft,
                  server: { ...globalDraft.server, open_browser: e.target.checked },
                })
              }
            />
            启动时自动打开浏览器
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              checked={globalDraft.server.exit_when_closed}
              onChange={(e) =>
                setGlobalDraft({
                  ...globalDraft,
                  server: { ...globalDraft.server, exit_when_closed: e.target.checked },
                })
              }
            />
            关闭页面后自动退出
          </label>
        </div>
        <p className="muted small">
          端口与开关在下次启动时生效；运行时可用 <code>--port</code> / <code>--no-browser</code> 覆盖。
          端口被占用时会自动向后尝试 50 个端口。
        </p>
      </section>
    </div>
  )
}
