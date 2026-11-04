import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { api, arxivDocId, arxivIdOf, errorText, looksLikeArxivId, type Doc, type Folder, type Hit, type Lang, type Language, type LlmStatus, type Meta, type Mode, type Status } from './api'
import { Reader } from './Reader'
import { PaperList } from './Papers'
import { SearchResults, type SearchState } from './Search'
import { AiChat, type ChatState } from './Ai'
import { RoadmapView, type RoadmapState } from './Roadmap'
import { FloatingAssistant, SelectionAsk, useAsk } from './Assistant'
import { ModelButton, ModelDialog, ProblemNotice } from './Model'
import { PatchNotes } from './PatchNotes'
import { setUiLang, t, tx, useT } from './i18n'
import { modKey, shortcut } from './platform'
import { posKey } from './position'
import { dragSplit, stored } from './resize'
import { applyTheme, initialTheme, type Theme } from './theme'
import logo from '../src-tauri/icons/128x128.png'

function ThemeButton() {
  useT()
  const [theme, setTheme] = useState<Theme>(initialTheme)
  const next: Theme = theme === 'dark' ? 'light' : 'dark'
  const label = next === 'dark' ? t('Switch to dark mode') : t('Switch to light mode')
  return (
    <button
      className="theme-btn"
      onClick={() => {
        applyTheme(next, true)
        setTheme(next)
      }}
      title={label}
      aria-label={label}
    >
      {theme === 'dark' ? (
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden="true">
          <circle cx="8" cy="8" r="3" />
          <path d="M8 1.5v1.3M8 13.2v1.3M1.5 8h1.3M13.2 8h1.3M3.4 3.4l.9.9M11.7 11.7l.9.9M3.4 12.6l.9-.9M11.7 4.3l.9-.9" />
        </svg>
      ) : (
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" aria-hidden="true">
          <path d="M13.5 9.6A5.6 5.6 0 0 1 6.4 2.5a5.6 5.6 0 1 0 7.1 7.1Z" />
        </svg>
      )}
    </button>
  )
}

const LANG_KEY = 'openpaper.lang'

/** The saved target language if still supported, else the first one matching the system language, else Korean. */
function pickLang(langs: Language[], saved: string | null): Lang {
  if (saved && langs.some(l => l.code === saved)) return saved
  const prefs = navigator.languages?.length ? navigator.languages : [navigator.language]
  for (const p of prefs) {
    const hit = langs.find(l => l.code === p.toLowerCase().split('-')[0])
    if (hit) return hit.code
  }
  return 'ko'
}

function PdfIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinejoin="round" aria-hidden="true">
      <path d="M9.5 1.75H4.25a1.5 1.5 0 0 0-1.5 1.5v9.5a1.5 1.5 0 0 0 1.5 1.5h7.5a1.5 1.5 0 0 0 1.5-1.5V5.5Z" />
      <path d="M9.5 1.75V5.5h3.75" />
      <path d="M5.5 9.25h5M5.5 11.5h3.25" strokeLinecap="round" />
    </svg>
  )
}

function SearchIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden="true">
      <circle cx="7" cy="7" r="4.75" />
      <path d="m10.5 10.5 3.75 3.75" />
    </svg>
  )
}

function GlobeIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden="true">
      <circle cx="8" cy="8" r="6.25" />
      <path d="M1.75 8h12.5M8 1.75c1.7 1.8 2.5 3.9 2.5 6.25S9.7 12.45 8 14.25C6.3 12.45 5.5 10.35 5.5 8S6.3 3.55 8 1.75Z" />
    </svg>
  )
}

/** Translation language: shows the native name; a transparent native select on top does the picking. */
function LangPicker({ langs, lang, onChange }: { langs: Language[]; lang: Lang; onChange: (lang: Lang) => void }) {
  useT()
  if (!langs.length) return null
  const current = langs.find(l => l.code === lang)
  return (
    <label className="lang-btn" title={t('Translation language')}>
      <GlobeIcon />
      <span>{current?.native ?? lang}</span>
      <select value={lang} onChange={e => onChange(e.target.value)} aria-label={t('Translation language')}>
        {langs.map(l => (
          <option key={l.code} value={l.code}>
            {l.native} · {l.name}
          </option>
        ))}
      </select>
    </label>
  )
}

const SIDE_W = 260
const SIDE_MIN = 200
const SIDE_MAX = 440

function SidebarIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden="true">
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" />
      <path d="M6 2.75v10.5" />
    </svg>
  )
}

const UPDATE_EVERY = 3 * 60 * 60 * 1000

/** Shown at the bottom of the sidebar only when a newer release exists. */
function UpdateButton() {
  useT()
  const [latest, setLatest] = useState<string | null>(null)
  const [installing, setInstalling] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [asking, setAsking] = useState(false)

  useEffect(() => {
    const check = () =>
      api
        .updateCheck()
        .then(u => setLatest(u.available ? u.latest ?? '' : null))
        .catch(() => {})
    void check()
    const timer = setInterval(check, UPDATE_EVERY)
    return () => clearInterval(timer)
  }, [])

  if (latest === null) return null
  const v = latest ? `v${latest.replace(/^v/, '')}` : ''

  const install = async () => {
    setAsking(false)
    setInstalling(true)
    setError(null)
    try {
      await api.updateInstall()
    } catch (err) {
      setError(errorText(err))
      setInstalling(false)
    }
  }

  const label = installing ? t('Updating…') : error ? t('Update failed: {error}', { error }) : v ? t('Update to {version}', { version: v }) : t('Update to the latest version')
  return (
    <>
      <button
        className={`update-btn${error ? ' failed' : ''}`}
        onClick={() => setAsking(true)}
        disabled={installing}
        title={installing ? label : error ? t('{label}. Click to try again', { label }) : t('{label}. Click to install it and reopen', { label })}
        aria-label={label}
      >
        {installing ? <span className="update-spin" aria-hidden="true" /> : <UpdateIcon />}
      </button>
      {asking && <ConfirmUpdate version={v} onCancel={() => setAsking(false)} onConfirm={() => void install()} />}
    </>
  )
}

/** Asks before the update closes the app. Enter updates, Escape or a click outside cancels. */
function ConfirmUpdate({ version, onCancel, onConfirm }: { version: string; onCancel: () => void; onConfirm: () => void }) {
  useT()
  const ok = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    ok.current?.focus()
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !e.isComposing) {
        e.preventDefault()
        e.stopPropagation()
        onCancel()
      }
    }
    window.addEventListener('keydown', key, true)
    return () => window.removeEventListener('keydown', key, true)
  }, [onCancel])
  return (
    <div className="modal-back" onMouseDown={e => e.target === e.currentTarget && onCancel()}>
      <div className="modal confirm-update" role="alertdialog" aria-modal="true" aria-labelledby="update-title" aria-describedby="update-text">
        <h2 id="update-title">{version ? t('Update to {version}?', { version }) : t('Update to the latest version?')}</h2>
        <p id="update-text">{t('OpenPaper closes, installs the update and opens again. A translation that is running stops.')}</p>
        <div className="confirm-actions">
          <button onClick={onCancel}>{t('Not now')}</button>
          <button ref={ok} className="primary" onClick={onConfirm}>{t('Update and restart')}</button>
        </div>
      </div>
    </div>
  )
}

function UpdateIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M8 12.5V3.5M4 7.5l4-4 4 4" />
    </svg>
  )
}

/** Translated sentences by unit id. */
type Tr = Record<string, string[]>

/** Keeps array identity for units that were already translated so their blocks skip re-rendering. */
function mergeTr(prev: Tr, next: Tr): Tr {
  let changed = Object.keys(prev).length !== Object.keys(next).length
  const out: Tr = {}
  for (const [k, v] of Object.entries(next)) {
    const old = prev[k]
    if (old && old.length === v.length && old.every((s, i) => s === v[i])) out[k] = old
    else {
      out[k] = v
      changed = true
    }
  }
  return changed ? out : prev
}

function onlyHeadingsMissing(doc: Doc, tr: Tr): boolean {
  let headingMissing = false
  for (const b of doc.blocks) {
    if (b.kind === 'heading') headingMissing ||= b.level > 1 && tr[b.unit.id] === undefined
    else if (b.kind === 'para' && tr[b.unit.id] === undefined) return false
    else if ((b.kind === 'figure' || b.kind === 'table') && b.caption && tr[b.caption.id] === undefined) return false
  }
  return headingMissing
}

const modeKey = (id: string) => `openpaper.mode.${id}`

/** Translate (or Retranslate in the new style) → pick a faithful translation or an easy explanation. */
function StartMenu({ label, title, onPick }: { label: string; title?: string; onPick: (mode: Mode) => void }) {
  useT()
  const [openMenu, setOpenMenu] = useState(false)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!openMenu) return
    const close = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpenMenu(false)
    }
    const esc = (e: KeyboardEvent) => e.key === 'Escape' && setOpenMenu(false)
    document.addEventListener('pointerdown', close)
    document.addEventListener('keydown', esc)
    return () => {
      document.removeEventListener('pointerdown', close)
      document.removeEventListener('keydown', esc)
    }
  }, [openMenu])
  const pick = (m: Mode) => {
    setOpenMenu(false)
    onPick(m)
  }
  return (
    <div className="start" ref={box}>
      <button className="primary" onClick={() => setOpenMenu(o => !o)} aria-haspopup="menu" aria-expanded={openMenu} title={title}>
        {label} <span className="caret" aria-hidden="true">▾</span>
      </button>
      {openMenu && (
        <div className="start-menu" role="menu">
          <button role="menuitem" onClick={() => pick('plain')}>
            <b>{t('Translate')}</b>
            <span>{t('Faithful, sentence by sentence')}</span>
          </button>
          <button role="menuitem" onClick={() => pick('easy')}>
            <b>{t('Make easy version')}</b>
            <span>{t('Hard terms in plain words')}</span>
          </button>
        </div>
      )}
    </div>
  )
}

export function App() {
  useT()
  const [papers, setPapers] = useState<Meta[]>([])
  const [folders, setFolders] = useState<Folder[]>([])
  const [doc, setDoc] = useState<Doc | null>(null)
  const [tr, setTr] = useState<Tr>({})
  const [status, setStatus] = useState<Status | null>(null)
  const [mode, setMode] = useState<Mode>('plain')
  // Whether the mode not on screen already has text or a running job, so both can be switched between.
  const [otherHas, setOtherHas] = useState(false)
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [search, setSearch] = useState<SearchState | null>(null)
  const [view, setView] = useState<'doc' | 'search' | 'ai' | 'roadmap'>('doc')
  const [roadmap, setRoadmap] = useState<RoadmapState | null>(null)
  const [chat, setChat] = useState<ChatState>({ messages: [], loading: false })
  const [fetchingId, setFetchingId] = useState<string | null>(null)
  const [langs, setLangs] = useState<Language[]>([])
  const [lang, setLang] = useState<Lang>(() => localStorage.getItem(LANG_KEY) ?? 'ko')
  useEffect(() => {
    api
      .languages()
      .then(list => {
        setLangs(list)
        setLang(pickLang(list, localStorage.getItem(LANG_KEY)))
      })
      .catch(() => {})
  }, [])
  // The interface follows the translation language.
  useLayoutEffect(() => setUiLang(lang), [lang])
  // Read inside `open` after its awaits, so a paper that finishes importing opens in the language picked meanwhile.
  const langRef = useRef(lang)
  langRef.current = lang
  const changeLang = (next: Lang) => {
    localStorage.setItem(LANG_KEY, next)
    setLang(next)
  }
  const [swapped, setSwapped] = useState(() => localStorage.getItem('openpaper.swapped') === '1')
  const toggleSwap = () =>
    setSwapped(s => {
      localStorage.setItem('openpaper.swapped', s ? '0' : '1')
      return !s
    })
  // Sidebar: drag its edge to resize, drag it narrow (or press ⌘B / Ctrl+B) to fold it away.
  const app = useRef<HTMLDivElement>(null)
  const [sideW, setSideW] = useState(() => stored('openpaper.sideWidth', SIDE_W, SIDE_MIN, SIDE_MAX))
  const [folded, setFolded] = useState(() => localStorage.getItem('openpaper.sideFolded') === '1')
  const fold = (f: boolean) => {
    localStorage.setItem('openpaper.sideFolded', f ? '1' : '0')
    setFolded(f)
  }
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (modKey(e) && !e.shiftKey && !e.altKey && e.key.toLowerCase() === 'b') {
        e.preventDefault()
        setFolded(f => {
          localStorage.setItem('openpaper.sideFolded', f ? '0' : '1')
          return !f
        })
      }
    }
    window.addEventListener('keydown', key)
    return () => window.removeEventListener('keydown', key)
  }, [])
  const resizeSide = (e: React.PointerEvent) => {
    const el = app.current
    if (!el) return
    let w = sideW
    let fold = false
    dragSplit(
      e,
      x => {
        fold = x < SIDE_MIN - 60
        w = Math.min(SIDE_MAX, Math.max(SIDE_MIN, Math.round(x)))
        el.style.setProperty('--side-w', `${fold ? 0 : w}px`)
        el.classList.toggle('folding', fold)
      },
      () => {
        // React owns this inline value too, so leave it at the settled width rather than removing it.
        el.style.setProperty('--side-w', `${fold ? sideW : w}px`)
        el.classList.remove('folding')
        if (fold) {
          localStorage.setItem('openpaper.sideFolded', '1')
          setFolded(true)
        } else {
          localStorage.setItem('openpaper.sideWidth', String(w))
          setSideW(w)
        }
      }
    )
  }
  const openSeq = useRef(0)
  /** The `open` call still in progress, so a language change can redo it in the new language. */
  const opening = useRef<{ id: string; start: boolean | 'redo'; want?: Mode; stay: boolean } | null>(null)
  const searchSeq = useRef(0)
  // Questions are about the paper on screen; search results and the welcome page ask without one.
  const shownDoc = view === 'doc' ? doc : null
  const assistant = useAsk(shownDoc?.id ?? null, lang)

  const refreshList = useCallback(
    () =>
      Promise.all([api.list().then(setPapers), api.folders().then(setFolders)]).catch(() => {}),
    []
  )
  // Show the change at once, then keep what Rust saved (it drops repeats and bad ids).
  const saveFolders = useCallback((next: Folder[]) => {
    setFolders(next)
    api.setFolders(next).then(setFolders).catch(e => setError(errorText(e)))
  }, [])
  useEffect(() => void refreshList(), [refreshList])

  // Translation and AI search need a working model backend (Claude Code by default); say so up
  // front instead of on first use.
  const [llm, setLlm] = useState<LlmStatus | null>(null)
  const [modelOpen, setModelOpen] = useState(false)
  const checkLlm = useCallback(() => void api.llmStatus().then(setLlm).catch(() => {}), [])
  const closeModel = useCallback(() => setModelOpen(false), [])
  useEffect(() => {
    void checkLlm()
    window.addEventListener('focus', checkLlm)
    return () => window.removeEventListener('focus', checkLlm)
  }, [checkLlm])
  // With backups saved, check now and then which backend answers, so the sidebar shows it.
  const watchBackup = (llm?.backups ?? 0) > 0
  useEffect(() => {
    if (!watchBackup) return
    const timer = window.setInterval(() => {
      void api
        .llmBackup()
        .then(b => setLlm(prev => (prev && JSON.stringify(prev.backup) !== JSON.stringify(b) ? { ...prev, backup: b } : prev)))
        .catch(() => {})
    }, 4000)
    return () => window.clearInterval(timer)
  }, [watchBackup])

  /**
   * Opens a paper in `want` mode (default: the one last read for it) in the current language, and optionally starts
   * that mode's job. `stay` reloads it in place without switching to the reader or scrolling.
   */
  const open = useCallback(async (id: string, startTranslation: boolean | 'redo', want?: Mode, stay = false) => {
    const seq = ++openSeq.current
    opening.current = { id, start: startTranslation, want, stay }
    setError(null)
    try {
      const d = await api.get(id)
      if (seq !== openSeq.current) return
      const lang = langRef.current
      const m: Mode = want ?? (localStorage.getItem(modeKey(id)) === 'easy' ? 'easy' : 'plain')
      localStorage.setItem(modeKey(id), m)
      setDoc(d)
      if (!stay) setView('doc')
      setMode(m)
      setTr({})
      setStatus(null)
      if (startTranslation === 'redo') await api.retranslate(id, lang)
      else if (startTranslation) await api.translate(id, m, lang)
      let s = await api.status(id, m, lang)
      if (seq !== openSeq.current) return
      // Papers translated before headings were translated (in any style): fill in just the headings.
      if (m === 'plain' && !s.running && onlyHeadingsMissing(d, s.units)) {
        await api.translate(id, m, lang)
        s = await api.status(id, m, lang)
      }
      const other = await api.status(id, m === 'plain' ? 'easy' : 'plain', lang)
      if (seq !== openSeq.current) return
      setOtherHas(other.running || Object.keys(other.units).length > 0)
      setStatus(s)
      setTr(s.units)
    } catch (e) {
      if (seq === openSeq.current) setError(errorText(e))
    } finally {
      if (seq === openSeq.current) opening.current = null
    }
  }, [])

  // A new target language reloads the paper in it: the one still opening if any (it would otherwise be
  // dropped or finish in the old language), else the loaded one, in place so search and AI views stay put.
  const openLang = useRef(lang)
  useEffect(() => {
    if (openLang.current === lang) return
    openLang.current = lang
    const o = opening.current
    if (o) void open(o.id, o.start === true, o.want, o.stay)
    else if (doc) void open(doc.id, false, mode, true)
    // Only a language change reloads; doc and mode are read as they are at that moment.
  }, [lang])

  // Poll while a translation job runs.
  useEffect(() => {
    if (!doc || !status?.running) return
    const id = doc.id
    const timer = setInterval(async () => {
      try {
        const s = await api.status(id, mode, lang)
        setStatus(s)
        setTr(prev => mergeTr(prev, s.units))
      } catch {
        /* next tick retries */
      }
    }, 1200)
    return () => clearInterval(timer)
  }, [doc, mode, lang, status?.running])

  const imported = async (meta: Meta | null) => {
    if (!meta) return
    await refreshList()
    await open(meta.id, true)
  }

  const runSearch = async (query: string, start: number) => {
    const seq = ++searchSeq.current
    setView('search')
    setSearch(prev =>
      start > 0 && prev ? { ...prev, loading: true, error: null } : { query, hits: [], total: 0, loading: true, error: null }
    )
    if (start === 0) document.querySelector('.main')?.scrollTo(0, 0)
    try {
      const page = await api.search(query, start)
      if (seq !== searchSeq.current) return
      setSearch(prev => {
        const kept = start > 0 && prev ? prev.hits : []
        const seen = new Set(kept.map(h => h.id))
        return { query, hits: [...kept, ...page.hits.filter(h => !seen.has(h.id))], total: page.total, loading: false, error: null }
      })
    } catch (err) {
      if (seq === searchSeq.current) setSearch(prev => (prev ? { ...prev, loading: false, error: errorText(err) } : prev))
    }
  }

  /** Opens a search or AI pick side by side; a paper not in the library is fetched and translation starts. */
  const openHit = async (hit: Hit, onError: (msg: string) => void) => {
    // A saved copy may carry a version (arxiv-2005.14165v4); match on the bare ID.
    const have = papers.find(p => arxivIdOf(p.id) === hit.id)
    if (have) return void open(have.id, false)
    if (fetchingId) return
    setFetchingId(hit.id)
    setError(null)
    try {
      await imported(await api.openArxiv(hit.id))
    } catch (err) {
      onError(errorText(err))
    } finally {
      setFetchingId(null)
    }
  }

  /** Shows the reading roadmap of a saved paper: the saved one, or a new one built now. */
  const roadmapSeq = useRef(0)
  const showRoadmap = (docId: string, seedTitle: string, fresh = false, onMade?: () => void) => {
    const asked = lang
    const seq = ++roadmapSeq.current
    setView('roadmap')
    setRoadmap({ docId, seedTitle, data: null, step: null, error: null })
    // Only the latest request may update the view (another paper, language or a newer build).
    const mine = (r: RoadmapState | null) => seq === roadmapSeq.current && r?.docId === docId
    api
      .roadmap(docId, asked, fresh, step => setRoadmap(r => (mine(r) ? { ...r!, step } : r)))
      .then(data => {
        setRoadmap(r => (mine(r) ? { ...r!, data, step: null } : r))
        onMade?.()
      })
      .catch(err => setRoadmap(r => (mine(r) ? { ...r!, step: null, error: errorText(err) } : r)))
  }

  /** The AI finder asked for a roadmap: from the paper it named (fetched if needed) or the open one. */
  const roadmapFromChat = async (seed: string) => {
    const say = (text: string) => setChat(c => ({ messages: [...c.messages, { role: 'assistant', text, picks: [], lang }], loading: false }))
    // A saved copy may carry a version (arxiv-2005.14165v4); match on the bare ID.
    const saved = seed && seed !== 'this' ? papers.find(p => arxivIdOf(p.id) === seed) : undefined
    let docId = seed === 'this' ? doc?.id ?? null : saved?.id ?? (seed ? arxivDocId(seed) : null)
    if (!docId) return say(t('Which paper should the roadmap start from? Open one or give its arXiv ID'))
    let title = saved?.title ?? (doc?.id === docId ? doc.title : '')
    if (seed !== 'this' && !saved) {
      const meta = await api.openArxiv(seed)
      await refreshList()
      docId = meta.id
      title = meta.title
    }
    setChat(c => ({ ...c, loading: false }))
    showRoadmap(docId, title, false, () => say(t('Made a reading roadmap from {title}', { title })))
  }

  const ask = async (text: string) => {
    const history = chat.messages
      .filter(m => m.role !== 'error')
      .map(m => ({
        role: m.role,
        text: m.role === 'assistant' && m.picks.length ? `${m.text}\n${m.picks.map(p => `${p.hit.id} ${p.hit.title}`).join('\n')}` : m.text,
      }))
    setChat(c => ({ messages: [...c.messages, { role: 'user', text }], loading: true }))
    const asked = lang
    try {
      const a = await api.recommend(text, history, asked)
      if (a.roadmap) return await roadmapFromChat(a.roadmap)
      if (a.roadmap === '' && a.picks.length) {
        // A topic: show the papers found, then build the roadmap from the first one.
        setChat(c => ({ messages: [...c.messages, { role: 'assistant', text: a.reply, picks: a.picks, lang: asked }], loading: true }))
        return await roadmapFromChat(a.picks[0]!.hit.id)
      }
      setChat(c => ({ messages: [...c.messages, { role: 'assistant', text: a.reply, picks: a.picks, lang: asked }], loading: false }))
    } catch (err) {
      setChat(c => ({ messages: [...c.messages, { role: 'error', text: errorText(err) }], loading: false }))
    }
  }
  const chatError = (msg: string) => setChat(c => ({ ...c, messages: [...c.messages, { role: 'error', text: msg }] }))

  const openWeb = (id: string) => void api.openArxivPage(id).catch(err => setError(errorText(err)))

  const submitArxiv = async (e: React.FormEvent) => {
    e.preventDefault()
    const q = input.trim()
    if (!q || busy) return
    if (!looksLikeArxivId(q)) return void runSearch(q, 0)
    setBusy(t('Fetching the paper from arXiv…'))
    setError(null)
    try {
      await imported(await api.openArxiv(input))
      setInput('')
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(null)
    }
  }

  const pickPdf = async () => {
    if (busy) return
    setBusy(t('Reading the PDF…'))
    setError(null)
    try {
      await imported(await api.openPdf())
    } catch (err) {
      setError(errorText(err))
    } finally {
      setBusy(null)
    }
  }

  const remove = async (id: string) => {
    await api.remove(id).catch(() => {})
    localStorage.removeItem(posKey(id))
    if (opening.current?.id === id) opening.current = null
    if (doc?.id === id) {
      openSeq.current++
      setDoc(null)
      setStatus(null)
    }
    await refreshList()
  }

  // Same units the Rust job translates: section headings, paragraphs and captions.
  const unitIds = useMemo(() => {
    const ids: number[] = []
    for (const b of doc?.blocks ?? []) {
      if ((b.kind === 'heading' && b.level > 1) || b.kind === 'para') ids.push(b.unit.id)
      else if ((b.kind === 'figure' || b.kind === 'table') && b.caption) ids.push(b.caption.id)
    }
    return ids
  }, [doc])
  const total = unitIds.length
  const translated = unitIds.filter(id => tr[id] !== undefined).length
  const langName = langs.find(l => l.code === lang)?.native ?? lang
  const remaining = total - translated

  return (
    <div className={`app${folded ? ' folded' : ''}`} ref={app} style={{ '--side-w': `${sideW}px` } as React.CSSProperties}>
      <aside className="side" aria-hidden={folded || undefined} inert={folded || undefined}>
        <div className="side-top" data-tauri-drag-region>
          <button className="side-fold" onClick={() => fold(true)} title={t('Hide sidebar ({shortcut})', { shortcut: shortcut('B') })} aria-label={t('Hide sidebar')}>
            <SidebarIcon />
          </button>
        </div>
        <div className="brand" data-tauri-drag-region>
          <img src={logo} alt="" width="22" height="22" draggable={false} />
          OpenPaper
        </div>
        <form className="import" onSubmit={submitArxiv}>
          <input
            value={input}
            onChange={e => setInput(e.target.value)}
            placeholder={t('Search arXiv · ID · URL')}
            spellCheck={false}
            disabled={!!busy}
            aria-label={t('arXiv search, ID or URL')}
          />
        </form>
        <div className="side-actions">
          <button className="pdf-btn" onClick={pickPdf} disabled={!!busy} title={t('Open a PDF file')} aria-label={t('Open a PDF file')}>
            <PdfIcon /> PDF
          </button>
          <button className={`ai-btn${view === 'ai' ? ' active' : ''}`} onClick={() => setView('ai')} title={t('Find papers with AI')} aria-label={t('Find papers with AI')}>
            <SearchIcon /> {t('AI search')}
            {chat.loading && <span className="ai-dot" aria-label={t('Searching')} />}
          </button>
        </div>
        {search && (
          <button className={`search-tab${view === 'search' ? ' active' : ''}`} onClick={() => setView('search')}>
            <span className="t">{t('Search: {query}', { query: search.query })}</span>
            <span className="n">{search.loading && !search.hits.length ? '…' : search.total.toLocaleString()}</span>
          </button>
        )}
        <PaperList
          papers={papers}
          folders={folders}
          onFolders={saveFolders}
          activeId={view === 'doc' ? doc?.id ?? null : null}
          onOpen={id => void open(id, false)}
          onRemove={id => void remove(id)}
          onRead={(id, read) => {
            setPapers(ps => ps.map(p => (p.id === id ? { ...p, read } : p)))
            api.setRead(id, read).catch(err => {
              setPapers(ps => ps.map(p => (p.id === id ? { ...p, read: !read } : p)))
              setError(errorText(err))
            })
          }}
        />
        <div className="side-foot">
          <ThemeButton />
          <LangPicker langs={langs} lang={lang} onChange={changeLang} />
          <ModelButton status={llm} onOpen={() => setModelOpen(true)} />
          <UpdateButton />
        </div>
      </aside>
      {!folded && (
        <div
          className="side-resize"
          role="separator"
          aria-orientation="vertical"
          aria-label={t('Sidebar width')}
          title={t('Drag to resize · double-click to reset')}
          onPointerDown={resizeSide}
          onDoubleClick={() => {
            localStorage.setItem('openpaper.sideWidth', String(SIDE_W))
            setSideW(SIDE_W)
          }}
        />
      )}

      <main className="main">
        <header className="bar" data-tauri-drag-region>
          {folded && (
            <button className="side-fold" onClick={() => fold(false)} title={t('Show sidebar ({shortcut})', { shortcut: shortcut('B') })} aria-label={t('Show sidebar')}>
              <SidebarIcon />
            </button>
          )}
          <div className="bar-title" data-tauri-drag-region>{view === 'search' ? t('arXiv search') : view === 'ai' ? t('Find papers with AI') : view === 'roadmap' ? t('Reading roadmap') : doc?.title ?? ''}</div>
          {view === 'doc' && doc && (
            <button className="bar-link" onClick={toggleSwap} title={swapped ? t('Put the original on the left') : t('Put the translation on the left')}>
              ⇄ {t('Swap sides')}
            </button>
          )}
          {view === 'doc' && doc && arxivIdOf(doc.id) && (
            <button className="bar-link" onClick={() => openWeb(arxivIdOf(doc.id)!)} title={t('Open arxiv.org in your browser')}>
              arxiv.org ↗
            </button>
          )}
          {view === 'doc' && doc && (
            <div className="progress">
              {(translated > 0 || status?.running || otherHas) && (
                <div className="mode-switch" role="group" aria-label={t('What the translation column shows')}>
                  {(['plain', 'easy'] as const).map(m => (
                    <button
                      key={m}
                      className={mode === m ? 'on' : ''}
                      aria-pressed={mode === m}
                      onClick={() => mode !== m && void open(doc.id, false, m)}
                      title={m === 'plain' ? t('Faithful translation') : t('Easy explanation with hard terms in plain words')}
                    >
                      {m === 'plain' ? t('Translation') : t('Easy')}
                    </button>
                  ))}
                </div>
              )}
              {status?.running && status.redo ? (
                <>
                  <span className="meter"><span style={{ width: `${status.total ? (status.done / status.total) * 100 : 0}%` }} /></span>
                  <span>{t('Retranslating {done}/{total}', { done: status.done, total: status.total })}</span>
                  <button onClick={() => void api.cancel(doc.id, mode, lang)}>{t('Stop')}</button>
                </>
              ) : status?.running ? (
                <>
                  <span className="meter"><span style={{ width: `${total ? (translated / total) * 100 : 0}%` }} /></span>
                  <span>{mode === 'easy' ? t('Making easy version {done}/{total}', { done: translated, total }) : t('Translating {done}/{total}', { done: translated, total })}</span>
                  <button onClick={() => void api.cancel(doc.id, mode, lang)}>{t('Stop')}</button>
                </>
              ) : translated === 0 && !otherHas ? (
                <StartMenu label={t('Translate')} onPick={m => void open(doc.id, true, m)} />
              ) : remaining > 0 && !status?.outdated ? (
                <button className="primary" onClick={() => void open(doc.id, true, mode)}>
                  {translated > 0 ? t('Continue ({n} left)', { n: remaining }) : mode === 'easy' ? t('Make easy version') : t('Translate')}
                </button>
              ) : status?.outdated ? (
                <StartMenu
                  label={t('Retranslate in the new style')}
                  title={t('Translates again from the start in a more readable style. The current translation stays until it finishes')}
                  onPick={m => void (m === 'plain' ? open(doc.id, 'redo', 'plain') : open(doc.id, true, 'easy'))}
                />
              ) : null}
              {!status?.running && (
                <>
                  <button className="summary-btn" onClick={() => showRoadmap(doc.id, doc.title)} title={t('What to read before and after this paper')}>
                    {t('Roadmap')}
                  </button>
                  <button className="summary-btn" onClick={() => assistant.summarize()} title={t('Summarize this paper')}>
                    <span className="spark" aria-hidden="true">✦</span> {t('Summarize')}
                  </button>
                </>
              )}
            </div>
          )}
        </header>
        {llm?.problem && !llm.backups && <ProblemNotice status={llm} onOpen={() => setModelOpen(true)} />}
        {(error || status?.error) && <div className="error" role="alert">{error ?? status?.error}</div>}
        {busy && <div className="notice">{busy}</div>}
        {view === 'ai' ? (
          <AiChat
            state={chat}
            saved={new Set(papers.map(p => arxivIdOf(p.id)).filter((x): x is string => !!x))}
            busyId={fetchingId}
            onAsk={q => void ask(q)}
            onTranslate={hit => void openHit(hit, chatError)}
            onWeb={hit => openWeb(hit.id)}
          />
        ) : view === 'roadmap' && roadmap ? (
          <RoadmapView
            state={roadmap}
            saved={new Set(papers.map(p => arxivIdOf(p.id)).filter((x): x is string => !!x))}
            busyId={fetchingId}
            onOpen={item => void openHit({ id: item.id, title: item.title, authors: [], summary: '', published: '', category: '' }, msg => setRoadmap(r => (r ? { ...r, error: msg } : r)))}
            onWeb={id => openWeb(id)}
            onRegenerate={() => showRoadmap(roadmap.docId, roadmap.seedTitle, true)}
            onBack={() => void open(roadmap.docId, false)}
          />
        ) : view === 'search' && search ? (
          <SearchResults
            state={search}
            saved={new Set(papers.map(p => arxivIdOf(p.id)).filter((x): x is string => !!x))}
            busyId={fetchingId}
            onOpen={hit => void openHit(hit, msg => setSearch(prev => (prev ? { ...prev, error: msg } : prev)))}
            onWeb={hit => openWeb(hit.id)}
            onMore={() => void runSearch(search.query, search.hits.length)}
          />
        ) : doc ? (
          <Reader doc={doc} tr={tr} lang={lang} swapped={swapped} trLabel={mode === 'easy' ? t('{language} · easy', { language: langName }) : langName} />
        ) : (
          !busy && (
            <div className="welcome">
              <h1>{t('Read papers side by side with a translation')}</h1>
              <p>
                {tx('In the sidebar, type {search} or {id}, or open a PDF.', {
                  search: <span className="nowrap">{t('a search (e.g. flash attention)')}</span>,
                  id: <span className="nowrap">{t('an arXiv ID (e.g. 1706.03762)')}</span>,
                })}
              </p>
              <p className="sub">{t('Translation runs on the Claude Code logged in on this computer, and results stay on this computer.')}</p>
            </div>
          )
        )}
      </main>
      {modelOpen && <ModelDialog onClose={closeModel} onChanged={checkLlm} />}
      <PatchNotes />
      {view !== 'ai' && (
        <>
          <FloatingAssistant ask={assistant} docTitle={shownDoc?.title ?? null} />
          <SelectionAsk busy={assistant.loading} resetKey={`${view}:${doc?.id ?? ''}`} onAsk={(q, quote) => assistant.send(q, quote)} />
        </>
      )}
    </div>
  )
}
