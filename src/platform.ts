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
