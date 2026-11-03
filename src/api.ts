import { Channel, invoke } from '@tauri-apps/api/core'

export type Unit = { id: number; sentences: string[] }

/** What the translation column shows: a faithful translation or an easy explanation of each sentence. */
export type Mode = 'plain' | 'easy'

/** Target language code, e.g. `ko`. The supported list comes from `api.languages()`. */
export type Lang = string

export type Language = { code: Lang; name: string; native: string }

export type Block =
  | { kind: 'heading'; level: number; unit: Unit }
  | { kind: 'para'; unit: Unit }
  | { kind: 'static'; text: string; code: boolean }
  | { kind: 'figure'; images: string[]; caption: Unit | null }
  | { kind: 'equation'; tex: string[] }
  | { kind: 'table'; caption: Unit | null; rows: string[][] }

export type Doc = { id: string; title: string; source: string; blocks: Block[]; math: string[] }

export type Meta = { id: string; title: string; source: string; addedAt: number; units: number }

/** A sidebar folder; `papers` are paper ids in order, and the first one stays visible when folded. */
export type Folder = { id: string; name: string; papers: string[]; open: boolean }

export type Status = {
  total: number
  done: number
  running: boolean
  error: string | null
  /** Retranslating the whole paper in the current style. */
  redo: boolean
  /** Saved translation is in an older style. */
  outdated: boolean
  units: Record<string, string[]>
}

/** Which model backend runs translation, questions, summaries and the AI finder. */
export type Provider = 'claude-code' | 'codex' | 'anthropic' | 'openai'

export type LlmSettings = {
  provider: Provider
  /** OpenAI-compatible preset id, e.g. `openrouter`; `custom` uses `baseUrl`. */
  preset: string
  baseUrl: string
  /** Model per slot (`claude-code`, `codex`, `anthropic`, `openai:<preset>`); missing means the default. */
  models: Record<string, string>
}

export type Preset = { id: string; name: string; base: string; needsKey: boolean }

export type LlmConfig = {
  settings: LlmSettings
  presets: Preset[]
  /** Last four characters of each saved key, by slot. */
  keyHints: Record<string, string>
}

export type LlmProblem = 'missing' | 'logged-out' | 'no-key' | 'no-model' | 'bad-url'

export type LlmStatus = { provider: Provider; label: string; problem: LlmProblem | null }

/** Key and model slot of the current choice, as in Rust's `Settings::slot`. */
export function slotOf(s: LlmSettings): string {
  return s.provider === 'openai' ? `openai:${s.preset}` : s.provider
}

export type Hit = {
  id: string
  title: string
  authors: string[]
  summary: string
  published: string
  category: string
}

export type SearchPage = { total: number; start: number; hits: Hit[] }

export const api = {
  languages: () => invoke<Language[]>('languages'),
  list: () => invoke<Meta[]>('library_list'),
  get: (id: string) => invoke<Doc>('library_get', { id }),
  remove: (id: string) => invoke<void>('library_remove', { id }),
  folders: () => invoke<Folder[]>('library_folders'),
  setFolders: (folders: Folder[]) => invoke<Folder[]>('library_set_folders', { folders }),
  openArxiv: (input: string) => invoke<Meta>('open_arxiv', { input }),
  search: (query: string, start: number) => invoke<SearchPage>('arxiv_search', { query, start }),
  llmStatus: () => invoke<LlmStatus>('llm_status'),
  llmSettings: () => invoke<LlmConfig>('llm_settings'),
  llmSetSettings: (settings: LlmSettings) => invoke<LlmConfig>('llm_set_settings', { settings }),
  /** Saves the API key of the current choice; an empty key removes it. */
  llmSetKey: (key: string) => invoke<LlmConfig>('llm_set_key', { key }),
  llmModels: () => invoke<string[]>('llm_models'),
  llmTest: () => invoke<{ reply: string; ms: number }>('llm_test'),
  recommend: (request: string, history: { role: string; text: string }[], lang: Lang) =>
    invoke<{ reply: string; picks: { hit: Hit; why: string }[] }>('ai_recommend', { request, history, lang }),
  openArxivPage: (id: string) => invoke<void>('open_arxiv_page', { id }),
  openPdf: () => invoke<Meta | null>('open_pdf'),
  translate: (id: string, mode: Mode, lang: Lang) => invoke<void>('translation_start', { id, mode, lang }),
  retranslate: (id: string, lang: Lang) => invoke<void>('translation_redo', { id, lang }),
  status: (id: string, mode: Mode, lang: Lang) => invoke<Status>('translation_status', { id, mode, lang }),
  cancel: (id: string, mode: Mode, lang: Lang) => invoke<void>('translation_cancel', { id, mode, lang }),
  updateCheck: () => invoke<{ current: string; latest: string | null; available: boolean }>('update_check'),
  /** Runs the installer detached; it quits this app, replaces it and reopens the new version. */
  updateInstall: () => invoke<void>('update_install'),
  /** Explainer post about a saved paper in `lang`: the saved one unless `fresh`, else a new one streamed over `onDelta`. */
  summarize: (docId: string, fresh: boolean, lang: Lang, onDelta: (text: string) => void) => {
    const onEvent = new Channel<{ kind: 'delta'; text: string }>()
    onEvent.onmessage = e => {
      if (e.kind === 'delta') onDelta(e.text)
    }
    return invoke<{ reply: string; saved: boolean }>('summarize_paper', { docId, fresh, lang, onEvent })
  },
  /** `onDelta` gets each piece of the answer as Claude writes it; the promise resolves with the full answer. */
  ask: (
    question: string,
    quote: string | null,
    docId: string | null,
    history: { role: 'user' | 'assistant'; text: string }[],
    lang: Lang,
    onDelta: (text: string) => void
  ) => {
    const onEvent = new Channel<{ kind: 'delta'; text: string }>()
    onEvent.onmessage = e => {
      if (e.kind === 'delta') onDelta(e.text)
    }
    return invoke<{ reply: string }>('ask_claude', { question, quote, docId, history, lang, onEvent })
  }
}

export function errorText(e: unknown): string {
  return typeof e === 'string' ? e : e instanceof Error ? e.message : 'Something went wrong'
}

/** Library id of an arXiv paper, mirroring `arxiv::fetch`. */
export const arxivDocId = (id: string) => `arxiv-${id.replace('/', '_')}`

/** arXiv ID of a library paper, or null for PDFs. */
export const arxivIdOf = (docId: string) => (docId.startsWith('arxiv-') ? docId.slice(6).replace('_', '/') : null)

/** True for inputs the importer accepts directly: an arXiv ID, `arXiv:` reference or arxiv.org URL. */
export function looksLikeArxivId(input: string): boolean {
  const s = input.trim()
  return (
    /^(arxiv:\s*)?\d{4}\.\d{4,5}(v\d+)?$/i.test(s) ||
    /^(arxiv:\s*)?[a-z-]+(\.[a-z]{2})?\/\d{7}(v\d+)?$/i.test(s) ||
    /^(https?:\/\/)?(www\.|export\.)?arxiv\.org\//i.test(s)
  )
}
