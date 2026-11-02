import { useEffect, useRef, useState } from 'react'
import { BAR_H } from './position'

// Matches are painted with the CSS Highlight API (WebKit 17.2+), so the paper's DOM is never touched.
type Registry = { set: (name: string, h: unknown) => void; delete: (name: string) => void }
const ALL = 'openpaper-find'
const CUR = 'openpaper-find-current'
const MAX = 1000

function paint(all: Range[], cur: Range | null) {
  const reg = (globalThis.CSS as unknown as { highlights?: Registry } | undefined)?.highlights
  const Ctor = (globalThis as unknown as { Highlight?: new (...r: Range[]) => unknown }).Highlight
  if (!reg || !Ctor) return
  if (all.length) reg.set(ALL, new Ctor(...all))
  else reg.delete(ALL)
  if (cur) reg.set(CUR, new Ctor(cur))
  else reg.delete(CUR)
}

/** Every case-insensitive match of `query` in the paper's text, in document order. */
function collect(root: HTMLElement, query: string): Range[] {
  const out: Range[] = []
  const q = query.toLowerCase()
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    // Skip KaTeX internals, the column labels, this bar's own counter, and headings repeated in English until they are translated.
    acceptNode: n =>
      n.parentElement?.closest('.katex, .cols-head, .untranslated, .find-dock, style') ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT,
  })
  for (let n = walker.nextNode(); n && out.length < MAX; n = walker.nextNode()) {
    const raw = n.nodeValue ?? ''
    const lower = raw.toLowerCase()
    // Lowercasing can change the length of a few characters; then match the text as it is.
    const [text, needle] = lower.length === raw.length ? [lower, q] : [raw, query]
    for (let i = text.indexOf(needle); i !== -1 && out.length < MAX; i = text.indexOf(needle, i + needle.length)) {
      const r = document.createRange()
      r.setStart(n, i)
      r.setEnd(n, i + needle.length)
      out.push(r)
    }
  }
  return out
}

function view(root: HTMLElement) {
  const main = root.closest<HTMLElement>('.main')
  const box = main?.getBoundingClientRect()
  return main && box ? { main, top: box.top + BAR_H, bottom: box.bottom } : null
}

/** Scrolls the match to a third of the way down unless it is already comfortably in view. */
function reveal(root: HTMLElement, r: Range) {
  const v = view(root)
  if (!v) return
  const box = r.getBoundingClientRect()
  if (box.top >= v.top + 40 && box.bottom <= v.bottom - 60) return
  v.main.scrollTop += box.top - (v.top + (v.bottom - v.top) / 3)
}

function Chevron({ up }: { up?: boolean }) {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
      <path d={up ? 'M4 10l4-4 4 4' : 'M4 6l4 4 4-4'} fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

type Props = {
  root: React.RefObject<HTMLDivElement | null>
  /** Changes when the text on screen may have changed (another paper, mode, language, or translation progress). */
  content: unknown
  /** The open paper; a new one restarts the search from the top of the view. */
  paper: string
  /** Bumped by ⌘F while the bar is already open, to focus and select the query again. */
  focus: number
  onClose: () => void
}

/** ⌘F bar over the reader: finds text in both columns. Enter for the next match, Shift+Enter for the previous. */
export function FindBar({ root, content, paper, focus, onClose }: Props) {
  const [query, setQuery] = useState('')
  const [ranges, setRanges] = useState<Range[]>([])
  const [idx, setIdx] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  const lastQuery = useRef('')

  useEffect(() => {
    input.current?.focus()
    input.current?.select()
  }, [focus])

  useEffect(() => {
    const el = root.current
    if (!el) return
    const found = query ? collect(el, query) : []
    setRanges(found)
    if (`${paper}\n${query}` !== lastQuery.current) {
      // A new query or paper starts at the first match in view, like a browser's find.
      lastQuery.current = `${paper}\n${query}`
      const v = view(el)
      const first = v ? found.findIndex(r => r.getBoundingClientRect().bottom > v.top) : 0
      const i = first === -1 ? 0 : first
      const r = found[i]
      setIdx(i)
      if (r) reveal(el, r)
    } else {
      setIdx(i => Math.min(i, Math.max(found.length - 1, 0)))
    }
  }, [query, paper, content, root])

  useEffect(() => paint(ranges, ranges[idx] ?? null), [ranges, idx])
  useEffect(() => () => paint([], null), [])

  const step = (d: number) => {
    const el = root.current
    if (!el || ranges.length === 0) return
    const i = (idx + d + ranges.length) % ranges.length
    const r = ranges[i]
    setIdx(i)
    if (r) reveal(el, r)
  }

  // ⌘G and ⇧⌘G step through matches from anywhere, as in other Mac apps.
  const stepRef = useRef(step)
  stepRef.current = step
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.metaKey && !e.altKey && e.key.toLowerCase() === 'g') {
        e.preventDefault()
        stepRef.current(e.shiftKey ? -1 : 1)
      }
    }
    window.addEventListener('keydown', key)
    return () => window.removeEventListener('keydown', key)
  }, [])

  const count = !query ? '' : ranges.length === 0 ? 'No matches' : `${idx + 1} of ${ranges.length}${ranges.length >= MAX ? '+' : ''}`

  return (
    <div className="find-dock">
      <div className="find" role="search">
        <input
          ref={input}
          value={query}
          onChange={e => setQuery(e.target.value)}
          onKeyDown={e => {
            // WebKit sends the key that ends an IME composition with keyCode 229 and isComposing already false.
            if (e.nativeEvent.isComposing || e.keyCode === 229) return
            if (e.key === 'Enter') {
              e.preventDefault()
              step(e.shiftKey ? -1 : 1)
            } else if (e.key === 'Escape') {
              e.preventDefault()
              e.stopPropagation()
              onClose()
            }
          }}
          placeholder="Find in paper"
          aria-label="Find in paper"
          spellCheck={false}
        />
        <span className="count" aria-live="polite">{count}</span>
        <button onClick={() => step(-1)} disabled={ranges.length === 0} title="Previous match (⇧⌘G)" aria-label="Previous match"><Chevron up /></button>
        <button onClick={() => step(1)} disabled={ranges.length === 0} title="Next match (⌘G)" aria-label="Next match"><Chevron /></button>
        <button onClick={onClose} title="Close (Esc)" aria-label="Close find">
          <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
          </svg>
        </button>
      </div>
    </div>
  )
}
