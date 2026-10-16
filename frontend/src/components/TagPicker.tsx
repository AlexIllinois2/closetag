import type { Tag } from '../types'

function depthMap(tags: Tag[]): Map<number, number> {
  const byId = new Map(tags.map((t) => [t.id, t]))
  const depth = new Map<number, number>()
  const compute = (id: number, seen: Set<number>): number => {
    if (depth.has(id)) return depth.get(id)!
    if (seen.has(id)) return 0
    seen.add(id)
    const t = byId.get(id)
    const d = t && t.parent_id != null ? compute(t.parent_id, seen) + 1 : 0
    depth.set(id, d)
    return d
  }
  for (const t of tags) compute(t.id, new Set())
  return depth
}

export function tagDepth(tags: Tag[], id: number | null): number {
  if (id == null) return 0
  return depthMap(tags).get(id) ?? 0
}

/**
 * Flatten the tag tree into depth-first order: every tag is immediately
 * followed by its own children (siblings sorted by name).
 *
 * A flat sort by depth level must NOT be used here — it pushes every
 * subtree below all root tags, so children end up rendered after the last
 * root and look like they belong to it (e.g. win's children showing up
 * under zip). Tags whose parent is missing from the list are appended at
 * the end so nothing ever disappears.
 */
export function sortTagsTree(tags: Tag[]): Tag[] {
  const byParent = new Map<number | null, Tag[]>()
  for (const t of tags) {
    const key = t.parent_id ?? null
    const arr = byParent.get(key) ?? []
    arr.push(t)
    byParent.set(key, arr)
  }
  for (const arr of byParent.values()) {
    arr.sort((a, b) => a.name.localeCompare(b.name))
  }
  const out: Tag[] = []
  const seen = new Set<number>()
  const walk = (parentId: number | null) => {
    for (const t of byParent.get(parentId) ?? []) {
      if (seen.has(t.id)) continue // guards against cycles in corrupt data
      seen.add(t.id)
      out.push(t)
      walk(t.id)
    }
  }
  walk(null)
  for (const t of tags) {
    if (!seen.has(t.id)) out.push(t)
  }
  return out
}

interface Props {
  tags: Tag[]
  selected: number[]
  onChange: (ids: number[]) => void
  disabled?: boolean
}

export default function TagPicker({ tags, selected, onChange, disabled }: Props) {
  const depths = depthMap(tags)
  const sorted = sortTagsTree(tags)

  const toggle = (id: number) => {
    if (disabled) return
    if (selected.includes(id)) onChange(selected.filter((x) => x !== id))
    else onChange([...selected, id])
  }

  if (sorted.length === 0) {
    return <div className="muted small">暂无标签，请先到「标签」页创建。</div>
  }

  return (
    <div className={`tag-picker ${disabled ? 'disabled' : ''}`}>
      {sorted.map((t) => (
        <label key={t.id} className="tag-picker-row" style={{ paddingLeft: 6 + (depths.get(t.id) ?? 0) * 18 }}>
          <input
            type="checkbox"
            checked={selected.includes(t.id)}
            disabled={disabled}
            onChange={() => toggle(t.id)}
          />
          <span>{t.name}</span>
          <span className="muted">（{t.item_count}）</span>
        </label>
      ))}
    </div>
  )
}
