import { NavLink } from 'react-router-dom'
import { api } from '../api'
import { useApp } from '../context'

export default function Header() {
  const { repos, currentRepoId, setCurrentRepoId, refreshRepos, currentRepo, toast } = useApp()

  const exportDb = () => {
    if (!currentRepoId) return
    const a = document.createElement('a')
    a.href = api.exportRepoUrl(currentRepoId)
    a.download = `${currentRepoId}.sqlite`
    document.body.appendChild(a)
    a.click()
    a.remove()
    toast('ok', `正在下载「${currentRepo?.name ?? currentRepoId}」的数据库`)
  }

  return (
    <header className="header">
      <div className="brand" title={currentRepo?.path ?? ''}>
        <span className="brand-mark">◆</span> closetag
      </div>
      <select
        className="repo-select"
        value={currentRepoId ?? ''}
        onChange={(e) => setCurrentRepoId(e.target.value)}
        title={currentRepo?.path ?? ''}
      >
        {repos.length === 0 && <option value="">（未配置仓库）</option>}
        {repos.map((r) => (
          <option key={r.id} value={r.id}>
            {r.name}
            {r.exists ? '' : '（路径不存在）'}
          </option>
        ))}
      </select>
      <nav className="nav">
        <NavLink to="/items">条目</NavLink>
        <NavLink to="/tags">标签</NavLink>
        <NavLink to="/groups">分组</NavLink>
        <NavLink to="/config">配置</NavLink>
      </nav>
      <div className="header-right">
        <button
          className="btn"
          onClick={exportDb}
          disabled={!currentRepoId}
          title="保存当前仓库的数据库文件（含标签与分组）"
        >
          导出数据库
        </button>
        <button className="btn" onClick={() => void refreshRepos()}>
          刷新
        </button>
      </div>
    </header>
  )
}
