import { useEffect, useRef, useState } from 'react'
import changelog from '../CHANGELOG.md?raw'

const SEEN_KEY = 'openpaper.seenVersion'
const SHOW_FOR = 9000

export type Notes = { version: string; items: string[] }

function parts(v: string): number[] {
  return v.split(/[-+]/)[0]!.split('.').map(n => Number(n) || 0)
}

/** True when version `a` is newer than `b`. */
export function newer(a: string, b: string): boolean {
  const x = parts(a)
  const y = parts(b)
  for (let i = 0; i < 3; i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0)
    if (d) return d > 0
  }
  return false
}

/** `## 1.2.3` sections of CHANGELOG.md with their `- ` items, newest first as written. */
export function parseChangelog(text: string): Notes[] {
  const out: Notes[] = []
  for (const line of text.split('\n')) {
    const head = /^##\s+v?(\d+\.\d+\.\d+)\s*$/.exec(line)
    if (head) {
      out.push({ version: head[1]!, items: [] })
      continue
    }
    const item = /^[-*]\s+(.+)$/.exec(line)
    const last = out[out.length - 1]
    if (item && last) last.items.push(item[1]!.trim())
  }
  return out
}

/** Items of every version after `seen` up to `current`, newest first. */
export function notesSince(text: string, seen: string, current: string): string[] {
  return parseChangelog(text)
    .filter(n => newer(n.version, seen) && !newer(n.version, current))
    .flatMap(n => n.items)
}

/**
 * What changed since the version this computer last ran, once. Nothing on a first install.
 * Someone updating from a version older than this feature has no seen version yet; any other
 * saved setting of the app shows they are not new, so they get this version's notes.
 */
function takeNotes(): Notes | null {
  const current = __APP_VERSION__
  let seen: string | null = null
  let known = false
  try {
    seen = localStorage.getItem(SEEN_KEY)
    for (let i = 0; i < localStorage.length && !known; i++) known = !!localStorage.key(i)?.startsWith('openpaper.')
    localStorage.setItem(SEEN_KEY, current)
  } catch {
    return null
  }
  if (!seen && !known) return null
  const items = seen ? notesSince(changelog, seen, current) : parseChangelog(changelog).find(n => n.version === current)?.items ?? []
  return items.length ? { version: current, items } : null
}

// Read once per launch, when the app loads and before it saves any setting of its own. The read
// also records this version as seen.
const pending = takeNotes()

/** "Updated to vX" card in the corner after an update. Closes by itself, or with the X. */
export function PatchNotes() {
  const [notes, setNotes] = useState<Notes | null>(pending)
  const [paused, setPaused] = useState(false)
  const [leaving, setLeaving] = useState(false)
  const timer = useRef(0)

  const close = () => {
    setLeaving(true)
    window.setTimeout(() => setNotes(null), 220)
  }

  useEffect(() => {
    if (!notes || paused || leaving) return
    timer.current = window.setTimeout(close, SHOW_FOR)
    return () => window.clearTimeout(timer.current)
  }, [notes, paused, leaving])

  if (!notes) return null
  return (
    <div
      className={`patch-notes${leaving ? ' leaving' : ''}${paused ? ' paused' : ''}`}
      role="status"
      aria-live="polite"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <div className="patch-head">
        <b>Updated to v{notes.version}</b>
        <button className="modal-close" onClick={close} aria-label="Close">
          <svg width="11" height="11" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>
        </button>
      </div>
      <ul>
        {notes.items.map((t, i) => (
          <li key={i}>{t}</li>
        ))}
      </ul>
      {/* Restarts with each pause, so the bar always shows a full countdown. */}
      {!paused && !leaving && <span className="patch-timer" style={{ animationDuration: `${SHOW_FOR}ms` }} aria-hidden="true" />}
    </div>
  )
}
