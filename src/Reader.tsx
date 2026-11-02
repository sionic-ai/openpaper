import { Fragment, memo, useEffect, useRef, useState } from 'react'
import type { Block, Doc, Lang, Unit } from './api'
import { inline, tex } from './render'
import { dragSplit, stored } from './resize'

/** Translated sentences by unit id. */
type Tr = Record<string, string[]>

function Sentences({ unit, side, math, tr, heading }: { unit: Unit; side: 'en' | 'tr'; math: string[]; tr: Tr; heading?: boolean }) {
  const translated = tr[String(unit.id)]
  if (side === 'tr' && !translated && heading) {
    // Until a heading is translated, repeat the original so both columns always show it.
    return <span className="untranslated" dangerouslySetInnerHTML={{ __html: inline(unit.sentences.join(' '), math) }} />
  }
  if (side === 'tr' && !translated) {
    return <span className="pending" aria-label="Not translated yet">{unit.sentences.map((_, i) => <i key={i} />)}</span>
  }
  const list = side === 'en' ? unit.sentences : translated!
  return (
    <>
      {list.map((s, i) =>
        s ? (
          <Fragment key={i}>
            <span className="s" data-s={`${unit.id}:${i}`} dangerouslySetInnerHTML={{ __html: inline(s, math) }} />{' '}
          </Fragment>
        ) : null
      )}
    </>
  )
}

function Pair({ unit, math, tr, lang, as: Tag, className }: { unit: Unit; math: string[]; tr: Tr; lang: Lang; as: 'p' | 'h1' | 'h2' | 'h3' | 'h4' | 'figcaption'; className?: string }) {
  return (
    <div className={`row ${className ?? ''}`}>
      <Tag className="en" lang="en"><Sentences unit={unit} side="en" math={math} tr={tr} /></Tag>
      <Tag className="tr" lang={lang}><Sentences unit={unit} side="tr" math={math} tr={tr} heading={Tag.startsWith('h')} /></Tag>
    </div>
  )
}

const BlockView = memo(function BlockView({ block, math, tr, lang }: { block: Block; math: string[]; tr: Tr; lang: Lang }) {
  switch (block.kind) {
    case 'heading': {
      const Tag = (['h1', 'h2', 'h3', 'h4'] as const)[Math.min(block.level, 4) - 1] ?? 'h4'
      if (block.level === 1) {
        // The paper title stays in English, once across both columns.
        return (
          <div className="row heading h1 wide-heading">
            <h1 lang="en" dangerouslySetInnerHTML={{ __html: inline(block.unit.sentences.join(' '), math) }} />
          </div>
        )
      }
      return <Pair unit={block.unit} math={math} tr={tr} lang={lang} as={Tag} className={`heading ${Tag}`} />
    }
    case 'para':
      return <Pair unit={block.unit} math={math} tr={tr} lang={lang} as="p" />
    case 'static':
      return block.code ? <pre className="wide code">{block.text}</pre> : <p className="wide static" dangerouslySetInnerHTML={{ __html: inline(block.text, math) }} />
    case 'equation':
      return <div className="wide equation" dangerouslySetInnerHTML={{ __html: block.tex.map(t => tex(t, true)).join('') }} />
    case 'figure':
      return (
        <figure className="wide-figure">
          {block.images.length > 0 && (
            <div className="images">{block.images.map((src, i) => <img key={i} src={src} alt="" loading="lazy" />)}</div>
          )}
          {block.caption && <Pair unit={block.caption} math={math} tr={tr} lang={lang} as="figcaption" className="caption" />}
        </figure>
      )
    case 'table':
      return (
        <figure className="wide-figure">
          {block.caption && <Pair unit={block.caption} math={math} tr={tr} lang={lang} as="figcaption" className="caption" />}
          <div className="table-wrap">
            <table>
              <tbody>
                {block.rows.map((row, r) => (
                  <tr key={r}>{row.map((cell, c) => <td key={c} dangerouslySetInnerHTML={{ __html: inline(cell, math) }} />)}</tr>
                ))}
              </tbody>
            </table>
          </div>
        </figure>
      )
  }
})

function unitIds(block: Block): number[] {
  switch (block.kind) {
    case 'heading':
    case 'para':
      return [block.unit.id]
    case 'figure':
    case 'table':
      return block.caption ? [block.caption.id] : []
    default:
      return []
  }
}

/** Only blocks whose translation changed re-render while a job streams in. */
function sliceTr(tr: Tr, ids: number[]): Tr {
  const out: Tr = {}
  for (const id of ids) {
    const t = tr[String(id)]
    if (t) out[String(id)] = t
  }
  return out
}

const TrSlice = memo(
  function TrSlice({ block, math, tr, lang }: { block: Block; math: string[]; tr: Tr; lang: Lang }) {
    return <BlockView block={block} math={math} tr={tr} lang={lang} />
  },
  (a, b) => a.block === b.block && a.lang === b.lang && unitIds(a.block).every(id => a.tr[String(id)] === b.tr[String(id)])
)

const GAP = 48
const SPLIT_MIN = 0.25
const SPLIT_MAX = 0.75

export function Reader({ doc, tr, lang, swapped, trLabel }: { doc: Doc; tr: Tr; lang: Lang; swapped: boolean; trLabel: string }) {
  const root = useRef<HTMLDivElement>(null)
  const style = useRef<HTMLStyleElement>(null)
  // Share of the width taken by the left column; drag the gap between the columns to change it.
  const [split, setSplit] = useState(() => stored('openpaper.split', 0.5, SPLIT_MIN, SPLIT_MAX))
  const resize = (e: React.PointerEvent) => {
    const el = root.current
    if (!el) return
    let r = split
    dragSplit(
      e,
      x => {
        const box = el.getBoundingClientRect()
        const pad = parseFloat(getComputedStyle(el).paddingLeft)
        const inner = box.width - pad * 2 - GAP
        r = Math.min(SPLIT_MAX, Math.max(SPLIT_MIN, (x - box.left - pad - GAP / 2) / inner))
        el.style.setProperty('--split', String(r))
      },
      () => {
        el.style.setProperty('--split', String(r))
        localStorage.setItem('openpaper.split', r.toFixed(3))
        setSplit(r)
      }
    )
  }

  // Hover highlight writes one CSS rule instead of re-rendering the paper.
  useEffect(() => {
    const el = root.current
    if (!el) return
    let current = ''
    const set = (key: string) => {
      if (key === current) return
      current = key
      if (style.current) style.current.textContent = key ? `.reader [data-s="${key}"]{background:var(--hl);box-shadow:0 0 0 2px var(--hl);}` : ''
    }
    const over = (e: MouseEvent) => {
      const s = (e.target as HTMLElement).closest<HTMLElement>('.s')
      set(s?.dataset.s ?? '')
    }
    const leave = () => set('')
    el.addEventListener('mouseover', over)
    el.addEventListener('mouseleave', leave)
    return () => {
      el.removeEventListener('mouseover', over)
      el.removeEventListener('mouseleave', leave)
    }
  }, [])

  return (
    <div className={swapped ? 'reader swapped' : 'reader'} ref={root} style={{ '--split': split } as React.CSSProperties}>
      <style ref={style} />
      <div
        className="col-resize"
        role="separator"
        aria-orientation="vertical"
        aria-label="Column widths"
        title="Drag to resize the columns · double-click to split evenly"
        onPointerDown={resize}
        onDoubleClick={() => {
          localStorage.setItem('openpaper.split', '0.5')
          setSplit(0.5)
        }}
      />
      <div className="cols-head">
        <span className="en">Original</span>
        <span className="tr">{trLabel}</span>
      </div>
      {doc.blocks.map((block, i) => (
        <TrSlice key={i} block={block} math={doc.math} tr={sliceTr(tr, unitIds(block))} lang={lang} />
      ))}
    </div>
  )
}
