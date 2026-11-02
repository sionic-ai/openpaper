import { getCurrentWindow } from '@tauri-apps/api/window'

export type Theme = 'light' | 'dark'
const KEY = 'openpaper.theme'

/** The saved choice, or the system appearance until the reader picks one. */
export function initialTheme(): Theme {
  const saved = localStorage.getItem(KEY)
  if (saved === 'light' || saved === 'dark') return saved
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

/** Paints the page and the native title bar. Transitions are paused so the whole window flips at once. */
export function applyTheme(theme: Theme, save = false) {
  const root = document.documentElement
  root.classList.add('theme-switching')
  root.dataset.theme = theme
  requestAnimationFrame(() => requestAnimationFrame(() => root.classList.remove('theme-switching')))
  if (save) localStorage.setItem(KEY, theme)
  if ('__TAURI_INTERNALS__' in window) void getCurrentWindow().setTheme(theme).catch(() => {})
}
