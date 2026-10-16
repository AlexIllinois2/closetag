import { useCallback, useEffect, useMemo, useState } from 'react'
import { NavLink, useSearchParams } from 'react-router-dom'
import { api } from '../api'
import { useApp } from '../context'
import type { Item, ItemsPage, ScanReport, Tag, TagGroup } from '../types'
import Modal from './Modal'
import TagPicker, { sortTagsTree, tagDepth } from './TagPicker'

const PAGE = 100

export default function ItemsPage() {
  const { currentRepoId, currentRepo, toast, refreshRepos } = useApp()
  const [searchParams] = useSearchParams()
  const [tags, setTags] = useState<Tag[]>([])
  const [groups, setGroups] = useState<TagGroup[]>([])
  const [page, setPage] = useState<ItemsPage | null>(null)
  const [tagFilter, setTagFilter] = useState<number | null>(null)
  const [groupFilter, setGroupFilter] = useState<number | null>(null)
  const [q, setQ] = useState('')
  const [debouncedQ, setDebouncedQ] = useState('')
  const [offset, setOffset] = useState(0)
  const [loading, setLoading] = useState(false)
  const [scanning, setScanning] = useState(false)
  const [editing, setEditing] = useState<Item | null>(null)
  const [creating, setCreating] = useState(false)
  const [groupById, setGroupById] = useState(true)

  // initial filters from URL (?tag=1&group=2)
  useEffect(() => {
    const t = searchParams.get('tag')
    const g = searchParams.get('group')
    if (t && Number.isFinite(Number(t))) setTagFilter(Number(t))
    if (g && Number.isFinite(Number(g))) setGroupFilter(Number(g))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    const timer = setTimeout(() => setDebouncedQ(q.trim()), 300)
    return () => clearTimeout(timer)
  }, [q])

  useEffect(() => {
    setOffset(0)
  }, [tagFilter, groupFilter, debouncedQ])

  const load = useCallback(async () => {
    if (!currentRepoId) return
    setLoading(true)
    try {
      const [items, tagList, groupList] = await Promise.all([
        api.items(currentRepoId, {
          tag: tagFilter ?? undefined,
          group: groupFilter ?? undefined,
          q: debouncedQ || undefined,
          limit: PAGE,
          offset,
        }),
        api.tags(currentRepoId),
        api.groups(currentRepoId),
      ])
      setPage(items)
      setTags(tagList)
      setGroups(groupList)
    } catch (e) {
      toast('err', String(e))
    } finally {
      setLoading(false)
    }
  }, [currentRepoId, tagFilter, groupFilter, debouncedQ, offset, toast])

  useEffect(() => {
    void load()
  }, [load])

  const doScan = async () => {
    if (!currentRepoId) return
    setScanning(true)
    try {
      const r: ScanReport = await api.scan(currentRepoId)
      toast(
        'ok',
        `扫描完成：${r.scanned} 个文件，新增 ${r.added}，更新 ${r.updated}，移除 ${r.removed}（${r.duration_ms}ms）`,
      )
      await load()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setScanning(false)
    }
  }

  const removeItem = async (item: Item) => {
    if (!currentRepoId) return
    if (!confirm(`删除条目「${item.path}」？其标签关联也会一并移除。`)) return
    try {
      await api.deleteItem(currentRepoId, item.pk)
      toast('ok', '已删除')
      await load()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    }
  }

  const removeTagFromItem = async (item: Item, tagId: number) => {
    if (!currentRepoId) return
    try {
      await api.removeItemTag(currentRepoId, item.pk, tagId)
      await load()
    } catch (e) {
      toast('err', String(e))
    }
  }

  const onSaved = async () => {
    await load()
    await refreshRepos()
  }

  const grouped = useMemo(() => {
    if (!page) return [] as [string, Item[]][]
    const m = new Map<string, Item[]>()
    for (const it of page.items) {
      const arr = m.get(it.id) ?? []
      arr.push(it)
      m.set(it.id, arr)
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0]))
  }, [page])

  if (!currentRepoId || !currentRepo) {
    return (
      <div className="empty">
        <h2>尚未选择仓库</h2>
        <p>请到「配置」页添加 repo_paths。</p>
      </div>
    )
  }
  if (!currentRepo.exists) {
    return (
      <div className="empty">
        <h2>仓库路径不存在</h2>
        <p>
          <code>{currentRepo.path}</code>
        </p>
        <p>请在「配置」中修正或移除该仓库。</p>
      </div>
    )
  }

  const renderRow = (item: Item, showId: boolean) => (
    <tr key={item.pk}>
      {showId && (
        <td className="col-id">
          <code>{item.id}</code>
        </td>
      )}
      <td className="col-path">
        <code>{item.path}</code>
      </td>
      <td>
        <div className="chips">
          {item.tags.map((t) => (
            <span key={t.id} className="chip">
              {t.name}
              <button title="移除标签" onClick={() => void removeTagFromItem(item, t.id)}>
                ✕
              </button>
            </span>
          ))}
          {item.tags.length === 0 && <span className="muted small">—</span>}
        </div>
      </td>
      <td className="col-actions">
        <div className="row-actions">
          <button className="btn small" onClick={() => setEditing(item)}>
            编辑
          </button>
          <button className="btn small danger" onClick={() => void removeItem(item)}>
            删除
          </button>
        </div>
      </td>
    </tr>
  )

  return (
    <div className="page">
      <div className="toolbar">
        <input
          className="input search"
          placeholder="搜索路径 / id…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
        <select
          className="input"
          value={tagFilter ?? ''}
          onChange={(e) => setTagFilter(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">全部标签</option>
          {sortTagsTree(tags).map((t) => (
            <option key={t.id} value={t.id}>
              {'　'.repeat(tagDepth(tags, t.parent_id))}
              {t.name}（{t.item_count}）
            </option>
          ))}
        </select>
        <select
          className="input"
          value={groupFilter ?? ''}
          onChange={(e) => setGroupFilter(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">全部分组</option>
          {groups.map((g) => (
            <option key={g.id} value={g.id}>
              {g.name}（{g.item_count}）
            </option>
          ))}
        </select>
        {(tagFilter != null || groupFilter != null) && (
          <button
            className="btn"
            onClick={() => {
              setTagFilter(null)
              setGroupFilter(null)
            }}
          >
            清除筛选
          </button>
        )}
        <label className="checkbox-label">
          <input
            type="checkbox"
            checked={groupById}
            onChange={(e) => setGroupById(e.target.checked)}
          />
          按 id 分组
        </label>
        <div className="toolbar-right">
          <button className="btn" onClick={() => void doScan()} disabled={scanning}>
            {scanning ? '扫描中…' : '扫描'}
          </button>
          <button className="btn primary" onClick={() => setCreating(true)}>
            ＋ 新建条目
          </button>
        </div>
      </div>

      <div className="stats-line muted">
        {page ? `共 ${page.total} 条` : '加载中…'}
        {currentRepo.last_scan && ` · 最近扫描 ${new Date(currentRepo.last_scan).toLocaleString()}`}
        {loading && ' · 加载中…'}
      </div>

      {page && page.items.length === 0 && (
        <div className="empty">
          <p>没有符合条件的条目。</p>
          {page.total === 0 && (
            <p className="muted">
              点击「扫描」把仓库里的文件收录进来，或手动「新建条目」。
            </p>
          )}
        </div>
      )}

      {groupById
        ? grouped.map(([gid, items]) => (
            <div key={gid} className="group-block">
              <div className="group-block-head">
                <code>{gid}</code>
                <span className="muted">{items.length} 个文件</span>
              </div>
              <table className="table">
                <thead>
                  <tr>
                    <th className="col-path">路径</th>
                    <th>标签</th>
                    <th className="col-actions">操作</th>
                  </tr>
                </thead>
                <tbody>{items.map((it) => renderRow(it, false))}</tbody>
              </table>
            </div>
          ))
        : (
            <table className="table">
              <thead>
                <tr>
                  <th className="col-id">ID</th>
                  <th className="col-path">路径</th>
                  <th>标签</th>
                  <th className="col-actions">操作</th>
                </tr>
              </thead>
              <tbody>{page?.items.map((it) => renderRow(it, true))}</tbody>
            </table>
          )}

      {page && page.total > page.items.length && (
        <div className="pagination">
          <button
            className="btn"
            disabled={offset === 0}
            onClick={() => setOffset(Math.max(0, offset - PAGE))}
          >
            上一页
          </button>
          <span className="muted">
            {offset + 1}–{offset + page.items.length} / {page.total}
          </span>
          <button
            className="btn"
            disabled={offset + page.items.length >= page.total}
            onClick={() => setOffset(offset + PAGE)}
          >
            下一页
          </button>
        </div>
      )}

      {creating && (
        <ItemModal
          repo={currentRepoId}
          tags={tags}
          item={null}
          onClose={() => setCreating(false)}
          onSaved={() => void onSaved()}
        />
      )}
      {editing && (
        <ItemModal
          repo={currentRepoId}
          tags={tags}
          item={editing}
          onClose={() => setEditing(null)}
          onSaved={() => void onSaved()}
        />
      )}
    </div>
  )
}

function ItemModal({
  repo,
  tags,
  item,
  onClose,
  onSaved,
}: {
  repo: string
  tags: Tag[]
  item: Item | null
  onClose: () => void
  onSaved: () => void
}) {
  const { toast } = useApp()
  const [path, setPath] = useState(item?.path ?? '')
  const [selected, setSelected] = useState<number[]>(item?.tags.map((t) => t.id) ?? [])
  const [saving, setSaving] = useState(false)

  const save = async () => {
    const p = path.trim()
    if (!p) {
      toast('err', '路径不能为空')
      return
    }
    setSaving(true)
    try {
      if (item) {
        await api.updateItem(repo, item.pk, { path: p, tags: selected })
      } else {
        await api.createItem(repo, { path: p, tags: selected })
      }
      toast('ok', '已保存')
      onSaved()
      onClose()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      title={item ? `编辑条目：${item.path}` : '新建条目'}
      onClose={onClose}
      footer={
        <>
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn primary" onClick={() => void save()} disabled={saving}>
            {saving ? '保存中…' : '保存'}
          </button>
        </>
      }
    >
      <label className="field">
        <span>路径（相对于仓库根目录）</span>
        <input
          className="input"
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder="src/utils/date-helper.ts"
          autoFocus
        />
      </label>
      <div className="field">
        <span>标签</span>
        <TagPicker tags={tags} selected={selected} onChange={setSelected} />
      </div>
      <p className="muted small">
        保存时会按当前 id_gen 规则重新生成 ID。也可以到 <NavLink to="/tags">标签页</NavLink> 管理标签树。
      </p>
    </Modal>
  )
}
