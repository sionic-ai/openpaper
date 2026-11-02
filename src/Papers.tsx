import { useEffect, useRef, useState } from 'react'
import type { Folder, Meta } from './api'

/**
 * The sidebar paper list with folders. Drag a paper (pointer events, so it does not depend on the
 * webview's native drag and drop) onto a folder to file it, onto a paper in a folder to put it
 * before that one, onto a loose paper to make a folder of the two, or onto the loose list to take
 * it out. A folded folder shows only its first paper, the folder's representative.
 */

/** Where a dragged paper would land; parsed from the `data-drop` attribute under the pointer. */
type Target =
  | { kind: 'folder'; folder: string; before: string | null }
  | { kind: 'pair'; paper: string }
  | { kind: 'new' }
  | { kind: 'loose' }

function parseTarget(raw: string | undefined): Target | null {
  if (!raw) return null
  const [kind, a, b] = raw.split(':')
  if (kind === 'f' && a) return { kind: 'folder', folder: a, before: b ?? null }
  if (kind === 'l' && a) return { kind: 'pair', paper: a }
  if (kind === 'new') return { kind: 'new' }
  if (kind === 'loose') return { kind: 'loose' }
  return null
}

const DRAG_START = 4

export function PaperList({
  papers,
  folders,
  onFolders,
  activeId,
  onOpen,
  onRemove,
}: {
  papers: Meta[]
  folders: Folder[]
  onFolders: (next: Folder[]) => void
  activeId: string | null
  onOpen: (id: string) => void
  onRemove: (id: string) => void
}) {
  const byId = new Map(papers.map(p => [p.id, p]))
  const filed = new Set(folders.flatMap(f => f.papers))
  const loose = papers.filter(p => !filed.has(p.id))
  const [drag, setDrag] = useState<{ id: string; x: number; y: number } | null>(null)
  const [over, setOver] = useState<string | null>(null)
  const [editing, setEditing] = useState<string | null>(null)
  const latest = useRef({ folders, onFolders })
  latest.current = { folders, onFolders }

  /** Removes `paper` from every folder, then applies `place` to a copy of the folders. */
  const regroup = (paper: string, place: (next: Folder[]) => void) => {
    const next = latest.current.folders.map(f => ({ ...f, papers: f.papers.filter(p => p !== paper) }))
    place(next)
    latest.current.onFolders(next)
  }

  const newFolder = (members: string[] = []) => {
    const id = `f${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`
    const next = latest.current.folders.map(f => ({ ...f, papers: f.papers.filter(p => !members.includes(p)) }))
    next.push({ id, name: 'New folder', papers: members, open: true })
    latest.current.onFolders(next)
    setEditing(id)
  }

  const dropOn = (paper: string, t: Target) => {
    if (t.kind === 'new') return newFolder([paper])
    if (t.kind === 'pair') return t.paper !== paper && newFolder([t.paper, paper])
    if (t.kind === 'loose') return filed.has(paper) && regroup(paper, () => {})
    if (t.before === paper) return
    regroup(paper, next => {
      const f = next.find(x => x.id === t.folder)
      if (!f) return
      const i = t.before ? f.papers.indexOf(t.before) : -1
      f.papers.splice(i < 0 ? f.papers.length : i, 0, paper)
    })
  }

  const startDrag = (e: React.PointerEvent, id: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest('.x')) return
    const x0 = e.clientX
    const y0 = e.clientY
    let moving = false
    let target: string | undefined
    const move = (ev: PointerEvent) => {
      if (!moving && Math.hypot(ev.clientX - x0, ev.clientY - y0) < DRAG_START) return
      moving = true
      const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>('[data-drop]')
      target = el?.dataset.drop
      setDrag({ id, x: ev.clientX, y: ev.clientY })
      setOver(target ?? null)
    }
    const stop = (ev: Event) => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', stop)
      window.removeEventListener('pointercancel', stop)
      window.removeEventListener('keydown', esc)
      setDrag(null)
      setOver(null)
      if (!moving) return
      // The release would otherwise click the row under the pointer and open that paper.
      const swallow = (c: MouseEvent) => {
        c.stopPropagation()
        c.preventDefault()
      }
      window.addEventListener('click', swallow, { capture: true, once: true })
      setTimeout(() => window.removeEventListener('click', swallow, { capture: true }), 0)
      const t = ev.type === 'pointerup' ? parseTarget(target) : null
      if (t) dropOn(id, t)
    }
    const esc = (k: KeyboardEvent) => {
      if (k.key === 'Escape') {
        target = undefined
        stop(new Event('pointercancel'))
      }
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', stop)
    window.addEventListener('pointercancel', stop)
    window.addEventListener('keydown', esc)
  }

  const row = (p: Meta, drop: string, extra = '', more = 0) => (
    <li
      key={p.id}
      data-drop={drop}
      className={[activeId === p.id && 'active', drag?.id === p.id && 'dragged', over === drop && drag?.id !== p.id && 'over', extra]
        .filter(Boolean)
        .join(' ')}
      onPointerDown={e => startDrag(e, p.id)}
    >
      <button className="paper" onClick={() => onOpen(p.id)} title={p.title}>
        <span className="t">{p.title}</span>
        <span className="src">
          {p.source.split('/').pop()}
          {more > 0 && <span className="others"> · +{more} more</span>}
        </span>
      </button>
      <button className="x" onClick={() => onRemove(p.id)} aria-label={`Delete ${p.title}`}>×</button>
    </li>
  )

  const setFolder = (id: string, patch: Partial<Folder>) =>
    onFolders(folders.map(f => (f.id === id ? { ...f, ...patch } : f)))

  return (
    <>
      <div className="list-label">
        <span>My papers</span>
        <button className="new-folder" onClick={() => newFolder()} title="New folder" aria-label="New folder">
          <FolderIcon plus />
        </button>
      </div>
      <ul className={`papers${drag ? ' dragging' : ''}${over === 'loose' ? ' over-loose' : ''}`} data-drop="loose">
        {folders.map(f => {
          const members = f.papers.map(id => byId.get(id)).filter((p): p is Meta => !!p)
          const shown = f.open ? members : members.slice(0, 1)
          const head = `f:${f.id}`
          return (
            <li key={f.id} className="folder" data-drop={head}>
              <div className={`folder-head${over === head ? ' over' : ''}`}>
                {editing === f.id ? (
                  <FolderName
                    name={f.name}
                    onDone={name => {
                      setEditing(null)
                      if (name !== null && name !== f.name) setFolder(f.id, { name })
                    }}
                  />
                ) : (
                  <button
                    className="folder-toggle"
                    onClick={() => setFolder(f.id, { open: !f.open })}
                    onDoubleClick={() => setEditing(f.id)}
                    aria-expanded={f.open}
                    title={`${f.name} · double-click to rename`}
                  >
                    <span className={`caret${f.open ? ' open' : ''}`} aria-hidden="true">›</span>
                    <FolderIcon />
                    <span className="name">{f.name}</span>
                    <span className="count">{members.length}</span>
                  </button>
                )}
                <button
                  className="x"
                  onClick={() => onFolders(folders.filter(x => x.id !== f.id))}
                  aria-label={`Delete folder ${f.name}`}
                  title="Delete folder (papers stay)"
                >
                  ×
                </button>
              </div>
              <ul className="folder-papers">
                {shown.map((p, i) =>
                  // Folded, a drop on the representative joins the folder at the end and keeps it in front.
                  row(p, f.open ? `f:${f.id}:${p.id}` : head, !f.open && i === 0 ? 'rep' : '', !f.open ? members.length - 1 : 0)
                )}
                {members.length === 0 && <li className="folder-empty">Drag papers here</li>}
              </ul>
            </li>
          )
        })}
        {loose.map(p => row(p, `l:${p.id}`))}
        {papers.length === 0 && <li className="empty">No papers yet</li>}
        {/* Last, so showing it while dragging does not move the rows under the pointer. */}
        {drag && (
          <li className={`drop-new${over === 'new' ? ' over' : ''}`} data-drop="new">
            <FolderIcon plus /> New folder
          </li>
        )}
      </ul>
      {drag && byId.get(drag.id) && (
        <div className="drag-ghost" style={{ left: drag.x + 12, top: drag.y + 8 }}>
          {byId.get(drag.id)!.title}
        </div>
      )}
    </>
  )
}

function FolderName({ name, onDone }: { name: string; onDone: (name: string | null) => void }) {
  const input = useRef<HTMLInputElement>(null)
  const done = useRef(false)
  useEffect(() => {
    input.current?.focus()
    input.current?.select()
  }, [])
  const finish = (value: string | null) => {
    if (done.current) return
    done.current = true
    onDone(value === null ? null : value.trim() || name)
  }
  return (
    <input
      ref={input}
      className="folder-name"
      defaultValue={name}
      maxLength={60}
      aria-label="Folder name"
      onKeyDown={e => {
        if (e.key === 'Enter' && !e.nativeEvent.isComposing) finish(e.currentTarget.value)
        if (e.key === 'Escape') finish(null)
      }}
      onBlur={e => finish(e.currentTarget.value)}
    />
  )
}

function FolderIcon({ plus = false }: { plus?: boolean }) {
  return (
    <svg className="folder-icon" width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path
        d="M1.75 4.25c0-.83.67-1.5 1.5-1.5h3l1.5 1.5h5c.83 0 1.5.67 1.5 1.5v6c0 .83-.67 1.5-1.5 1.5h-9.5c-.83 0-1.5-.67-1.5-1.5z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinejoin="round"
      />
      {plus && <path d="M8 7v4M6 9h4" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" />}
    </svg>
  )
}
