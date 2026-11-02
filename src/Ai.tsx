import { useEffect, useRef, useState } from 'react'
import type { Hit, Lang } from './api'

export type Pick = { hit: Hit; why: string }

export type ChatMsg =
  | { role: 'user'; text: string }
  /** `lang` is the language the reply was asked in. */
  | { role: 'assistant'; text: string; picks: Pick[]; lang: Lang }
  | { role: 'error'; text: string }

export type ChatState = { messages: ChatMsg[]; loading: boolean }

type Props = {
  state: ChatState
  saved: Set<string>
  busyId: string | null
  onAsk: (text: string) => void
  onTranslate: (hit: Hit) => void
  onWeb: (hit: Hit) => void
}

const EXAMPLES = ['Surveys on storing knowledge in LLM parameters', 'The original RAG paper and recent improvements', 'Faster attention for long contexts']

function authors(list: string[]): string {
  if (list.length <= 2) return list.join(', ')
  return `${list[0]} et al.`
}

export function AiChat({ state, saved, busyId, onAsk, onTranslate, onWeb }: Props) {
  const [text, setText] = useState('')
  const end = useRef<HTMLDivElement>(null)
  const box = useRef<HTMLTextAreaElement>(null)
  const { messages, loading } = state

  // Braces: newer engines return a Promise from scrollIntoView, which React would take for a cleanup.
  useEffect(() => {
    end.current?.scrollIntoView({ block: 'end' })
  }, [messages.length, loading])
  useEffect(() => box.current?.focus(), [])

  const send = (q: string) => {
    const t = q.trim()
    if (!t || loading) return
    onAsk(t)
    setText('')
  }

  return (
    <section className="ai">
      <div className="ai-log">
        {messages.length === 0 && (
          <div className="ai-empty">
            <h1>What are you looking for?</h1>
            <p>Describe a topic or question and Claude picks papers from arXiv. Click <b>Translate</b> on a pick to read it side by side.</p>
            <div className="ai-examples">
              {EXAMPLES.map(e => (
                <button key={e} onClick={() => send(e)}>{e}</button>
              ))}
            </div>
          </div>
        )}
        {messages.map((m, i) =>
          m.role === 'user' ? (
            <div key={i} className="msg user"><div className="bubble">{m.text}</div></div>
          ) : m.role === 'error' ? (
            <div key={i} className="msg bot"><div className="bubble error-bubble">{m.text}</div></div>
          ) : (
            <div key={i} className="msg bot">
              <div className="bubble">
                <p className="reply" lang={m.lang}>{m.text}</p>
                {m.picks.map(({ hit, why }) => (
                  <div key={hit.id} className="pick">
                    <div className="pick-title">{hit.title}</div>
                    <div className="pick-meta">
                      <span className="pick-id">arXiv:{hit.id}</span>
                      <span>{authors(hit.authors)}</span>
                      <span>{hit.published.slice(0, 4)}</span>
                    </div>
                    {why && <p className="pick-why" lang={m.lang}>{why}</p>}
                    <div className="pick-actions">
                      <button className="go" onClick={() => onTranslate(hit)} disabled={busyId !== null}>
                        {busyId === hit.id ? 'Fetching…' : saved.has(hit.id) ? 'Open from library' : 'Translate'}
                      </button>
                      <button className="web" onClick={() => onWeb(hit)} title={`https://arxiv.org/abs/${hit.id}`}>arxiv.org ↗</button>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )
        )}
        {loading && (
          <div className="msg bot"><div className="bubble typing">Searching arXiv<span className="dots" /></div></div>
        )}
        <div ref={end} />
      </div>
      <form
        className="ai-input"
        onSubmit={e => {
          e.preventDefault()
          send(text)
        }}
      >
        <textarea
          ref={box}
          value={text}
          rows={1}
          onChange={e => setText(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault()
              send(text)
            }
          }}
          placeholder="e.g. papers on compressing the transformer KV cache"
          aria-label="Papers to find"
          spellCheck={false}
        />
        <button type="submit" disabled={loading || !text.trim()}>Send</button>
      </form>
    </section>
  )
}
