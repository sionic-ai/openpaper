import { useEffect, useRef, useState } from 'react'
import type { AiMode, Hit, Lang } from './api'
import { t, tx, useT } from './i18n'

export type Pick = { hit: Hit; why: string }

export type ChatMsg =
  | { role: 'user'; text: string }
  /** `lang` is the language the reply was asked in. */
  | { role: 'assistant'; text: string; picks: Pick[]; lang: Lang }
  | { role: 'error'; text: string }

export type ChatState = { messages: ChatMsg[]; loading: boolean }

type Props = {
  state: ChatState
  mode: AiMode
  onMode: (mode: AiMode) => void
  saved: Set<string>
  busyId: string | null
  onAsk: (text: string) => void
  onTranslate: (hit: Hit) => void
  onWeb: (hit: Hit) => void
}

const examples = (mode: AiMode) =>
  mode === 'roadmap'
    ? [t('I want to research how LLMs store knowledge'), t('A reading roadmap for this paper'), t('Where to start with diffusion models')]
    : [t('Surveys on storing knowledge in LLM parameters'), t('The original RAG paper and recent improvements'), t('Faster attention for long contexts')]

function authors(list: string[]): string {
  if (list.length <= 2) return list.join(', ')
  return t('{name} et al.', { name: list[0]! })
}

export function AiChat({ state, mode, onMode, saved, busyId, onAsk, onTranslate, onWeb }: Props) {
  useT()
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
      <div className="ai-modes" role="tablist" aria-label={t('AI search')}>
        {(['search', 'roadmap'] as const).map(m => (
          <button
            key={m}
            role="tab"
            aria-selected={mode === m}
            className={mode === m ? 'on' : ''}
            onClick={() => {
              if (mode !== m) onMode(m)
              box.current?.focus()
            }}
          >
            {m === 'search' ? t('Find papers') : t('Roadmap')}
          </button>
        ))}
      </div>
      <div className="ai-log">
        {messages.length === 0 && (
          <div className="ai-empty">
            {mode === 'roadmap' ? (
              <>
                <h1>{t('What do you want to research?')}</h1>
                <p>{t('Name a topic or a paper and get papers to read in order. "This paper" means the one you have open.')}</p>
              </>
            ) : (
              <>
                <h1>{t('What are you looking for?')}</h1>
                <p>{tx('Describe a topic or question and Claude picks papers from arXiv. Click {translate} on a pick to read it side by side.', { translate: <b>{t('Translate')}</b> })}</p>
              </>
            )}
            <div className="ai-examples">
              {examples(mode).map(e => (
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
                        {busyId === hit.id ? t('Fetching…') : saved.has(hit.id) ? t('Open from library') : t('Translate')}
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
          <div className="msg bot"><div className="bubble typing">{t('Searching arXiv')}<span className="dots" /></div></div>
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
          placeholder={mode === 'roadmap' ? t('e.g. I want to research how LLMs store knowledge') : t('e.g. papers on compressing the transformer KV cache')}
          aria-label={mode === 'roadmap' ? t('What to research') : t('Papers to find')}
          spellCheck={false}
        />
        <button type="submit" disabled={loading || !text.trim()}>{t('Send')}</button>
      </form>
    </section>
  )
}
