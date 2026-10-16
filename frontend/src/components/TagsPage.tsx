import { useCallback, useEffect, useState } from 'react'
import { NavLink } from 'react-router-dom'
import { api } from '../api'
import { useApp } from '../context'
import type { Tag } from '../types'
import Modal from './Modal'
import { sortTagsTree, tagDepth } from './TagPicker'

export default function TagsPage() {
  const { currentRepoId, currentRepo, toast, refreshRepos } = useApp()
  const [tags, setTags] = useState<Tag[]>([])
  const [editing, setEditing] = useState<{ tag: Tag | null; parentId: number | null } | null>(null)

  const load = useCallback(async () => {
    if (!currentRepoId) return
    try {
      setTags(await api.tags(currentRepoId))
    } catch (e) {
      toast('err', String(e))
    }
  }, [currentRepoId, toast])

  useEffect(() => {
    void load()
  }, [load])

  const remove = async (t: Tag) => {
    if (!currentRepoId) return
    if (!confirm(`删除标签「${t.name}」？\n子标签会移到根级，条目上该标签的关联会被移除。`)) return
    try {
      await api.deleteTag(currentRepoId, t.id)
      toast('ok', '已删除')
      await load()
      await refreshRepos()
    } catch (e) {
      toast('err', String(e))
    }
  }

  const childrenOf = (parentId: number | null) =>
    tags
      .filter((t) => (t.parent_id ?? null) === parentId)
      .sort((a, b) => a.name.localeCompare(b.name))

  const renderNode = (t: Tag, depth: number) => (
    <div key={t.id}>
      <div className="tree-row" style={{ paddingLeft: 8 + depth * 20 }}>
        <span className="tree-name">{t.name}</span>
        <span className="muted tree-count" title="条目数">
          {t.item_count}
        </span>
        {t.description && (
          <span className="muted tree-desc" title={t.description}>
            {t.description}
          </span>
        )}
        <div className="row-actions">
          <NavLink className="btn small" to={`/items?tag=${t.id}`}>
            条目
          </NavLink>
          <button className="btn small" onClick={() => setEditing({ tag: null, parentId: t.id })}>
            ＋子级
          </button>
          <button className="btn small" onClick={() => setEditing({ tag: t, parentId: t.parent_id })}>
            编辑
          </button>
          <button className="btn small danger" onClick={() => void remove(t)}>
            删除
          </button>
        </div>
      </div>
      {childrenOf(t.id).map((c) => renderNode(c, depth + 1))}
    </div>
  )

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
        <h2 className="page-title">标签</h2>
        <div className="toolbar-right">
          <button className="btn primary" onClick={() => setEditing({ tag: null, parentId: null })}>
            ＋ 新建根标签
          </button>
        </div>
      </div>
      <div className="card">
        {tags.length === 0 ? (
          <div className="empty">
            <p>还没有标签，先创建一个吧。</p>
          </div>
        ) : (
          childrenOf(null).map((t) => renderNode(t, 0))
        )}
      </div>
      {editing && (
        <TagModal
          repo={currentRepoId}
          tags={tags}
          tag={editing.tag}
          parentId={editing.parentId}
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

function TagModal({
  repo,
  tags,
  tag,
  parentId,
  onClose,
  onSaved,
}: {
  repo: string
  tags: Tag[]
  tag: Tag | null
  parentId: number | null
  onClose: () => void
  onSaved: () => Promise<void>
}) {
  const { toast } = useApp()
  const [name, setName] = useState(tag?.name ?? '')
  const [parent, setParent] = useState<number | null>(tag ? tag.parent_id : parentId)
  const [desc, setDesc] = useState(tag?.description ?? '')
  const [saving, setSaving] = useState(false)

  // exclude the tag itself and its descendants from the parent dropdown
  const excluded = new Set<number>()
  if (tag) {
    const collect = (id: number) => {
      excluded.add(id)
      for (const t of tags) {
        if (t.parent_id === id) collect(t.id)
      }
    }
    collect(tag.id)
  }

  const save = async () => {
    if (!name.trim()) {
      toast('err', '名称不能为空')
      return
    }
    setSaving(true)
    try {
      if (tag) {
        await api.updateTag(repo, tag.id, {
          name: name.trim(),
          parent_id: parent,
          description: desc,
        })
      } else {
        await api.createTag(repo, {
          name: name.trim(),
          parent_id: parent,
          description: desc,
        })
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
      title={tag ? `编辑标签「${tag.name}」` : '新建标签'}
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
          placeholder="utils"
          autoFocus
        />
      </label>
      <label className="field">
        <span>父标签</span>
        <select
          className="input"
          value={parent ?? ''}
          onChange={(e) => setParent(e.target.value ? Number(e.target.value) : null)}
        >
          <option value="">（根级）</option>
          {sortTagsTree(tags)
            .filter((t) => !excluded.has(t.id))
            .map((t) => (
              <option key={t.id} value={t.id}>
                {'　'.repeat(tagDepth(tags, t.parent_id))}
                {t.name}
              </option>
            ))}
        </select>
      </label>
      <label className="field">
        <span>描述</span>
        <textarea
          className="input"
          rows={3}
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
          placeholder="可选"
        />
      </label>
    </Modal>
  )
}
