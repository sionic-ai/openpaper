import { Fragment, type ReactNode, useSyncExternalStore } from 'react'

/**
 * UI language. The app shows its interface in the translation language picked in the sidebar.
 * The English text is the key: `t('Couldn't start the translation')` looks the sentence up in
 * `src/locales/<code>.json` and falls back to the English when a language has no entry.
 * Placeholders are `{name}`; keep them unchanged in every locale file.
 */
type Dict = Record<string, string>

const files = import.meta.glob<Dict>('./locales/*.json', { eager: true, import: 'default' })
const DICTS: Record<string, Dict> = Object.fromEntries(
  Object.entries(files).map(([path, dict]) => [/([\w-]+)\.json$/.exec(path)![1]!, dict])
)

let current = 'en'
const listeners = new Set<() => void>()

/** Switches the interface language; unknown codes show English. */
export function setUiLang(code: string) {
  const next = DICTS[code] ? code : 'en'
  if (next === current) return
  current = next
  document.documentElement.lang = next
  listeners.forEach(f => f())
}

export function uiLang(): string {
  return current
}

function fill(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text
  return text.replace(/\{(\w+)\}/g, (all, k: string) => (k in vars ? String(vars[k]) : all))
}

/** The UI text for `english` in the current language, with `{name}` placeholders filled in. */
export function t(english: string, vars?: Record<string, string | number>): string {
  return fill(DICTS[current]?.[english] || english, vars)
}

/** Like `t`, for text with elements in it: `tx('Run {cmd} in Terminal', { cmd: <code>…</code> })`. */
export function tx(english: string, parts: Record<string, ReactNode>): ReactNode {
  const text = DICTS[current]?.[english] || english
  return text.split(/(\{\w+\})/).map((piece, i) => {
    const key = /^\{(\w+)\}$/.exec(piece)?.[1]
    return <Fragment key={i}>{key && key in parts ? parts[key] : piece}</Fragment>
  })
}

/**
 * An error message from the Rust side in the current language. Messages are fixed English
 * sentences, sometimes followed by `: detail` from a server or CLI; the detail stays as it is.
 */
export function tError(message: string): string {
  const dict = DICTS[current]
  if (!dict) return message
  if (dict[message]) return dict[message]
  const at = message.indexOf(': ')
  if (at > 0 && dict[message.slice(0, at)]) return `${dict[message.slice(0, at)]}: ${message.slice(at + 2)}`
  return message
}

function subscribe(f: () => void) {
  listeners.add(f)
  return () => {
    listeners.delete(f)
  }
}
const snapshot = () => current

/** Re-renders the calling component when the interface language changes. Returns `t`. */
export function useT(): typeof t {
  useSyncExternalStore(subscribe, snapshot)
  return t
}
