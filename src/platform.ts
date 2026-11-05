// The app runs on macOS and Windows. Shortcuts use ⌘ on macOS and Ctrl on Windows.

export const isMac = /Mac/i.test(navigator.platform || navigator.userAgent)

/** True when the platform's command modifier (⌘ or Ctrl) is held, and only that one of the two. */
export function modKey(e: KeyboardEvent): boolean {
  return isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey
}

/** A shortcut label for `key`: `⌘F` / `⇧⌘G` on macOS, `Ctrl+F` / `Ctrl+Shift+G` on Windows. */
export function shortcut(key: string, shift = false): string {
  return isMac ? `${shift ? '⇧' : ''}⌘${key}` : `Ctrl+${shift ? 'Shift+' : ''}${key}`
}

/** True for a click with the platform's command modifier (⌘ on macOS, Ctrl on Windows): open in a new tab. */
export function modClick(e: { metaKey: boolean; ctrlKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey
}

/**
 * True when `e` is the letter key `letter` (lowercase a-z). With a Korean, Japanese or other non-Latin
 * input source the key reports its own character (⌘ㅅ for ⌘T), so fall back to the physical key then.
 */
export function isLetter(e: KeyboardEvent, letter: string): boolean {
  const k = e.key.toLowerCase()
  if (k === letter) return true
  return k.length === 1 && k.charCodeAt(0) > 0x7f && e.code === `Key${letter.toUpperCase()}`
}
