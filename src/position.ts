// Where each paper was last read, as the original-language sentence at the top of the view
// plus how far into that sentence the view was. Stored per paper in localStorage.

/** Height of the sticky header at the top of `.main`. */
export const BAR_H = 48

type Pos = { s: string; dy: number }

export const posKey = (id: string) => `openpaper.pos.${id}`

/**
 * The column whose sentences mark the position: the original, unless only the translation is shown.
 * Both columns use the same sentence keys, so a position saved in one finds its place in the other.
 */
const side = (reader: HTMLElement) => (reader.classList.contains('only-tr') ? 'tr' : 'en')

export function loadPos(id: string): Pos | null {
  try {
    const v = JSON.parse(localStorage.getItem(posKey(id)) ?? 'null') as Partial<Pos> | null
    return v && typeof v.s === 'string' && typeof v.dy === 'number' ? { s: v.s, dy: v.dy } : null
  } catch {
    return null
  }
}

/**
 * The sentence at the top of the view, null when the paper is scrolled to its start, or undefined when
 * no sentence is near the top (a long table or code listing), in which case the last position is kept.
 */
export function readPos(main: HTMLElement, reader: HTMLElement): Pos | null | undefined {
  if (main.scrollTop < 24) return null
  const c = side(reader)
  const col = reader.querySelector(`.row > .${c}`)
  if (!col) return undefined
  const top = main.getBoundingClientRect().top + BAR_H
  const x = col.getBoundingClientRect().left + 8
  // Step down past figures, equations and gaps until a sentence is under the point.
  // elementsFromPoint sees through the floating panels that may cover the reader.
  for (let y = top + 4; y < top + 600; y += 12) {
    const s = document.elementsFromPoint(x, y).find((e): e is HTMLElement => e instanceof HTMLElement && e.matches(`.${c} .s`))
    if (s?.dataset.s) return { s: s.dataset.s, dy: top - s.getBoundingClientRect().top }
  }
  return undefined
}

/** Scrolls so the view is where `pos` was. Returns false when that sentence is not in the paper. */
export function applyPos(main: HTMLElement, reader: HTMLElement, pos: Pos): boolean {
  const s = reader.querySelector(`.${side(reader)} [data-s="${CSS.escape(pos.s)}"]`)
  if (!s) return false
  const r = s.getBoundingClientRect()
  // The layout may have changed since (window width, translation filling in); stay inside the sentence.
  const dy = Math.min(Math.max(pos.dy, 0), Math.max(r.height - 20, 0))
  main.scrollTop += r.top + dy - (main.getBoundingClientRect().top + BAR_H)
  return true
}
