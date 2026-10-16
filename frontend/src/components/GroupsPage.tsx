import { useCallback, useEffect, useState } from 'react'
import { NavLink } from 'react-router-dom'
import { api } from '../api'
import { useApp } from '../context'
import type { Tag, TagGroup } from '../types'
import Modal from './Modal'
import TagPicker from './TagPicker'

export default function GroupsPage() {
  const { currentRepoId, currentRepo, toast, refreshRepos } = useApp()
  const [groups, setGroups] = useState<TagGroup[]>([])
  const [tags, setTags] = useState<Tag[]>([])
  const [editing, setEditing] = useState<{ group: TagGroup | null } | null>(null)

  const load = useCallback(async () => {
    if (!currentRepoId) return
    try {
      const [g, t] = await Promise.all([api.groups(currentRepoId), api.tags(currentRepoId)])
      setGroups(g)
      setTags(t)
    } catch (e) {
      toast('err', String(e))
    }
  }, [currentRepoId, toast])

  useEffect(() => {
    void load()
  }, [load])

  const remove = async (g: TagGroup) => {
    if (!currentRepoId) return
    if (!confirm(`删除分组「${g.name}」？`)) return
    try {
      await api.deleteGroup(currentRepoId, g.id)
      toast('ok', '已删除')
      await load()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    }
  }

  const tagName = (id: number) => tags.find((t) => t.id === id)?.name ?? `#${id}`

  if (!currentRepoId || !currentRepo) {
    return (
      <div className="empty">
        <h2>尚未选择仓库</h2>
        <p>请到「配置」页添加 repo_paths。</p>
      </div>
    )
  }

  return (
    <div className="page">
      <div className="toolbar">
        <h2 className="page-title">分组</h2>
        <div className="toolbar-right">
          <button className="btn primary" onClick={() => setEditing({ group: null })}>
            ＋ 新建分组
          </button>
        </div>
      </div>
      <p className="muted small intro">
        分组按标签规则筛选条目：命中 includes 中任意标签（includes 为空表示全部），且不命中 excludes
        中任何标签。
      </p>

      {groups.length === 0 ? (
        <div className="empty">
          <p>还没有分组。</p>
        </div>
      ) : (
        <div className="group-list">
          {groups.map((g) => (
            <div key={g.id} className="card group-card">
              <div className="group-card-head">
                <strong>{g.name}</strong>
                <span className="badge">{g.item_count} 条</span>
                <div className="row-actions">
                  <NavLink className="btn small" to={`/items?group=${g.id}`}>
                    查看条目
                  </NavLink>
                  <button className="btn small" onClick={() => setEditing({ group: g })}>
                    编辑
                  </button>
                  <button className="btn small danger" onClick={() => void remove(g)}>
                    删除
                  </button>
                </div>
              </div>
              <div className="group-rules">
                <div>
                  <span className="muted small">includes：</span>
                  {g.includes === null ? (
                    <span className="chip chip-info">全部条目</span>
                  ) : g.includes.length === 0 ? (
                    <span className="muted small">无（不匹配任何条目）</span>
                  ) : (
                    g.includes.map((id) => (
                      <span key={id} className="chip">
                        {tagName(id)}
                      </span>
                    ))
                  )}
                </div>
                <div>
                  <span className="muted small">excludes：</span>
                  {g.excludes.length === 0 ? (
                    <span className="muted small">无</span>
                  ) : (
                    g.excludes.map((id) => (
                      <span key={id} className="chip chip-warn">
                        {tagName(id)}
                      </span>
                    ))
                  )}
                </div>
                {g.description && <div className="muted small">{g.description}</div>}
              </div>
            </div>
          ))}
        </div>
      )}

      {editing && (
        <GroupModal
          repo={currentRepoId}
          tags={tags}
          group={editing.group}
          onClose={() => setEditing(null)}
          onSaved={async () => {
            await load()
            await refreshRepos()
          }}
        />
      )}
    </div>
  )
}

function GroupModal({
  repo,
  tags,
  group,
  onClose,
  onSaved,
}: {
  repo: string
  tags: Tag[]
  group: TagGroup | null
  onClose: () => void
  onSaved: () => Promise<void>
}) {
  const { toast } = useApp()
  const [name, setName] = useState(group?.name ?? '')
  const [all, setAll] = useState(group ? group.includes === null : true)
  const [includes, setIncludes] = useState<number[]>(group?.includes ?? [])
  const [excludes, setExcludes] = useState<number[]>(group?.excludes ?? [])
  const [desc, setDesc] = useState(group?.description ?? '')
  const [saving, setSaving] = useState(false)

  const save = async () => {
    if (!name.trim()) {
      toast('err', '名称不能为空')
      return
    }
    setSaving(true)
    try {
      const payload = {
        name: name.trim(),
        includes: all ? null : includes,
        excludes,
        description: desc,
      }
      if (group) {
        await api.updateGroup(repo, group.id, payload)
      } else {
        await api.createGroup(repo, payload)
      }
      toast('ok', '已保存')
      await onSaved()
      onClose()
    } catch (e) {
      toast('err', String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <Modal
      title={group ? `编辑分组「${group.name}」` : '新建分组'}
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
        <span>名称</span>
        <input
          className="input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="前端相关"
          autoFocus
        />
      </label>
      <label className="checkbox-label">
        <input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} />
        包含全部条目（includes = null）
      </label>
      <div className={`field ${all ? 'dimmed' : ''}`}>
        <span>includes（命中任意一个）</span>
        <TagPicker tags={tags} selected={includes} onChange={setIncludes} disabled={all} />
      </div>
      <div className="field">
        <span>excludes（命中任意一个则排除）</span>
        <TagPicker tags={tags} selected={excludes} onChange={setExcludes} />
      </div>
      <label className="field">
        <span>描述</span>
        <textarea
          className="input"
          rows={2}
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
          placeholder="可选"
        />
      </label>
    </Modal>
  )
}
