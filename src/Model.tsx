import { useEffect, useRef, useState } from 'react'
import { api, errorText, slotOf, type LlmConfig, type LlmSettings, type LlmStatus, type ModelOption, type Provider } from './api'
import { Logo, type LogoId } from './logos'
import { isMac } from './platform'

const PROVIDERS: { id: Provider; name: string; sub: string; logo: LogoId }[] = [
  { id: 'claude-code', name: 'Claude Code', sub: 'Your Claude Code login', logo: 'claude' },
  { id: 'codex', name: 'Codex', sub: 'Your ChatGPT login in Codex', logo: 'codex' },
  { id: 'anthropic', name: 'Anthropic API', sub: 'Your Anthropic API key', logo: 'anthropic' },
  { id: 'openai', name: 'OpenAI-compatible', sub: 'OpenAI, OpenRouter, DeepSeek, Gemini, Ollama, or any URL', logo: 'openai' },
]

const PRESET_LOGO: Record<string, LogoId> = {
  openai: 'openai',
  openrouter: 'openrouter',
  deepseek: 'deepseek',
  gemini: 'gemini',
  ollama: 'ollama',
  custom: 'custom',
}

function logoOf(provider: Provider, preset: string): LogoId {
  if (provider === 'openai') return PRESET_LOGO[preset] ?? 'custom'
  return PROVIDERS.find(p => p.id === provider)?.logo ?? 'custom'
}

/** Models offered before any list loads. The first one is the default when none is picked. */
const KNOWN: Partial<Record<Provider, ModelOption[]>> = {
  'claude-code': [
    { id: 'sonnet', name: 'Sonnet', note: 'Balanced. Good for most papers' },
    { id: 'opus', name: 'Opus', note: 'Most capable. Slower and uses more of your plan' },
    { id: 'haiku', name: 'Haiku', note: 'Fastest and lightest on your plan' },
  ],
  codex: [{ id: '', name: 'Codex default', note: 'The model Codex picks for your login' }],
  anthropic: [
    { id: 'claude-sonnet-5-5', name: 'Claude Sonnet 5.5', note: 'Balanced. Good for most papers' },
    { id: 'claude-opus-5-5', name: 'Claude Opus 5.5', note: 'Most capable. Costs more per paper' },
    { id: 'claude-haiku-5-5', name: 'Claude Haiku 5.5', note: 'Fastest and cheapest' },
  ],
}

/** The model in effect: the saved one, or the default for backends that have one. */
function effectiveModel(provider: Provider, saved: string): string {
  return saved || (provider === 'codex' ? '' : KNOWN[provider]?.[0]?.id ?? '')
}

/** Short name of the current model for the sidebar and the dialog header. */
function modelName(status: LlmStatus): string {
  const known = KNOWN[status.provider]?.find(m => m.id === status.model)
  if (known?.name) return known.name
  return status.model || (status.provider === 'codex' ? 'Codex default' : 'No model picked')
}

/** Sidebar footer button showing the current backend's mark and model; opens Model settings. */
export function ModelButton({ status, onOpen }: { status: LlmStatus | null; onOpen: () => void }) {
  const text = status ? modelName(status) : 'Model'
  const title = status ? `Model: ${status.label}, ${text}. Click to change` : 'Model settings'
  return (
    <button className={`lang-btn model-btn${status?.problem ? ' warn' : ''}`} onClick={onOpen} title={title}>
      {status ? <Logo id={logoOf(status.provider, status.preset)} size={14} /> : null}
      <span>{text}</span>
    </button>
  )
}

/** One line for the notice above the reader when the chosen backend can't run. */
export function ProblemNotice({ status, onOpen }: { status: LlmStatus; onOpen: () => void }) {
  const shell = isMac ? 'Terminal' : 'PowerShell'
  const open = (
    <button className="notice-link" onClick={onOpen}>
      Model settings
    </button>
  )
  const { provider, problem, label } = status
  let text: React.ReactNode = null
  if (provider === 'claude-code' && problem === 'missing') {
    text = isMac ? (
      <>Claude Code isn't installed, so translation and AI search are off. In Terminal, run <code>curl -fsSL https://claude.ai/install.sh | bash</code>, then <code>claude auth login</code>. Or pick another model in {open}.</>
    ) : (
      <>Claude Code isn't installed, so translation and AI search are off. In PowerShell, run <code>irm https://claude.ai/install.ps1 | iex</code>, then <code>claude auth login</code>. Or pick another model in {open}.</>
    )
  } else if (provider === 'claude-code' && problem === 'logged-out') {
    text = <>Claude Code isn't logged in. Run <code>claude auth login</code> in {shell}. The app checks again when you come back to this window.</>
  } else if (provider === 'codex' && problem === 'missing') {
    text = <>Codex isn't installed, so translation and AI search are off. Install the Codex CLI or the ChatGPT app, or pick another model in {open}.</>
  } else if (provider === 'codex' && problem === 'logged-out') {
    text = <>Codex isn't logged in. Run <code>codex login</code> in {shell}. The app checks again when you come back to this window.</>
  } else if (problem === 'no-key') {
    text = <>{label} needs an API key. Add it in {open}.</>
  } else if (problem === 'no-model') {
    text = <>Pick a {label} model in {open}.</>
  } else if (problem === 'bad-url') {
    text = <>Add the API address in {open}. It must start with https://.</>
  }
  return text ? <div className="notice claude-notice" role="status">{text}</div> : null
}

type Test = { state: 'idle' } | { state: 'running' } | { state: 'ok'; text: string } | { state: 'error'; text: string }

/** Model settings: which backend, its key and model, and a test call. Every change saves at once. */
export function ModelDialog({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const [config, setConfig] = useState<LlmConfig | null>(null)
  const [status, setStatus] = useState<LlmStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [key, setKey] = useState('')
  const [model, setModel] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [listed, setListed] = useState<ModelOption[]>([])
  const [listing, setListing] = useState(false)
  const [listError, setListError] = useState<string | null>(null)
  const [reload, setReload] = useState(0)
  const [test, setTest] = useState<Test>({ state: 'idle' })
  const box = useRef<HTMLDivElement>(null)
  // The latest settings, including saves still in flight, so two quick changes (a field blur and
  // a click) never overwrite each other. Saves run one after another.
  const latest = useRef<LlmSettings | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())

  const refresh = () => {
    void api.llmStatus().then(setStatus).catch(() => {})
    onChanged()
  }
  const apply = (next: LlmConfig) => {
    latest.current = next.settings
    setConfig(next)
    setModel(next.settings.models[slotOf(next.settings)] ?? '')
    setBaseUrl(next.settings.baseUrl)
    setTest({ state: 'idle' })
    refresh()
  }
  useEffect(() => {
    api.llmSettings().then(apply).catch(e => setError(errorText(e)))
    // Load once when the dialog opens.
  }, [])

  // Escape closes only this dialog: listen first (capture) and stop it there, so the assistant
  // panel and the selection bubble underneath stay open.
  const closeRef = useRef<() => void>(() => {})
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      // Escape in the model name field only closes that field.
      if (e.key === 'Escape' && !e.isComposing && !(e.target as Element | null)?.closest?.('.model-other')) {
        e.preventDefault()
        e.stopPropagation()
        closeRef.current()
      }
    }
    window.addEventListener('keydown', key, true)
    box.current?.focus()
    return () => window.removeEventListener('keydown', key, true)
  }, [])

  const s = config?.settings
  const slot = s ? slotOf(s) : ''
  const preset = config?.presets.find(p => p.id === s?.preset)
  const isApi = s?.provider === 'anthropic' || s?.provider === 'openai'
  const needsKey = s?.provider === 'anthropic' || (s?.provider === 'openai' && (preset?.needsKey ?? true))
  const hint = config?.keyHints[slot]

  // The model list: Codex reads its own list from disk; APIs list theirs once they can be reached.
  const canList =
    !!s && (s.provider === 'codex' || (isApi && (!needsKey || !!hint) && !(s.provider === 'openai' && s.preset === 'custom' && !s.baseUrl)))
  const listKey = s ? `${s.provider}|${s.preset}|${s.baseUrl}|${hint ?? ''}|${reload}` : ''
  useEffect(() => {
    setListed([])
    setListError(null)
    if (!canList) return
    let live = true
    setListing(true)
    api
      .llmModels()
      .then(list => live && setListed(list))
      .catch(e => live && setListError(errorText(e)))
      .finally(() => live && setListing(false))
    return () => {
      live = false
    }
  }, [listKey, canList])

  const save = (patch: Partial<LlmSettings>) => {
    const base = latest.current
    if (!base) return
    const next = { ...base, ...patch }
    latest.current = next
    setError(null)
    queue.current = queue.current
      .then(() => api.llmSetSettings(next))
      .then(apply)
      .catch(e => setError(errorText(e)))
  }
  const saveModel = (value: string) => {
    const base = latest.current
    if (!base) return
    const at = slotOf(base)
    if ((base.models[at] ?? '') === value.trim()) return
    const models = { ...base.models }
    if (value.trim()) models[at] = value.trim()
    else delete models[at]
    save({ models })
  }
  const saveBaseUrl = (value: string) => {
    if (latest.current && value.trim() !== latest.current.baseUrl) save({ baseUrl: value.trim() })
  }
  // The typed fields save on blur, which doesn't happen when the dialog closes; save them first.
  const close = () => {
    if (latest.current?.provider === 'openai' && latest.current.preset === 'custom') saveBaseUrl(baseUrl)
    onClose()
  }
  closeRef.current = close
  const saveKey = (value: string) => {
    setError(null)
    queue.current = queue.current
      .then(() => api.llmSetKey(value))
      .then(next => {
        setKey('')
        apply(next)
      })
      .catch(e => setError(errorText(e)))
  }
  const runTest = () => {
    setTest({ state: 'running' })
    api
      .llmTest()
      .then(r => setTest({ state: 'ok', text: `Works. Replied in ${(r.ms / 1000).toFixed(1)} s` }))
      .catch(e => setTest({ state: 'error', text: errorText(e) }))
  }

  return (
    <div className="modal-back" onMouseDown={e => e.target === e.currentTarget && close()}>
      <div className="modal model-dialog" role="dialog" aria-modal="true" aria-labelledby="model-title" ref={box} tabIndex={-1}>
        <div className="modal-head">
          <h2 id="model-title">Model</h2>
          <button className="modal-close" onClick={close} aria-label="Close">
            <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>
          </button>
        </div>
        <p className="modal-sub">What runs translation, summaries, questions and the AI finder.</p>
        {status && (
          <div className={`now-using${status.problem ? ' warn' : ''}`} aria-live="polite">
            <Logo id={logoOf(status.provider, status.preset)} size={20} />
            <span>
              <small>Now using</small>
              <b>{status.label}</b>
              <span className="sep" aria-hidden="true">·</span>
              <span className="now-model">{modelName(status)}</span>
            </span>
          </div>
        )}
        {s && (
          <>
            <div className="providers" role="radiogroup" aria-label="Model backend">
              {PROVIDERS.map(p => (
                <button
                  key={p.id}
                  role="radio"
                  aria-checked={s.provider === p.id}
                  className={s.provider === p.id ? 'on' : ''}
                  onClick={() => s.provider !== p.id && save({ provider: p.id })}
                >
                  <span className="provider-name">
                    <Logo id={p.logo} size={18} />
                    <b>{p.name}</b>
                  </span>
                  <span className="provider-sub">{p.sub}</span>
                </button>
              ))}
            </div>

            {s.provider === 'openai' && (
              <div className="field">
                <span className="field-label" id="llm-preset">Service</span>
                <div className="presets" role="radiogroup" aria-labelledby="llm-preset">
                  {config.presets.map(p => (
                    <button
                      key={p.id}
                      role="radio"
                      aria-checked={s.preset === p.id}
                      className={s.preset === p.id ? 'on' : ''}
                      onClick={() => s.preset !== p.id && save({ preset: p.id })}
                    >
                      <Logo id={PRESET_LOGO[p.id] ?? 'custom'} size={16} />
                      <span>{p.name}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
            {s.provider === 'openai' && s.preset === 'custom' && (
              <div className="field">
                <label htmlFor="llm-base">API address</label>
                <input
                  id="llm-base"
                  value={baseUrl}
                  placeholder="https://example.com/v1"
                  spellCheck={false}
                  onChange={e => setBaseUrl(e.target.value)}
                  onBlur={() => saveBaseUrl(baseUrl)}
                  onKeyDown={e => e.key === 'Enter' && !e.nativeEvent.isComposing && (e.target as HTMLInputElement).blur()}
                />
              </div>
            )}

            {isApi && needsKey && (
              <div className="field">
                <label htmlFor="llm-key">API key</label>
                {hint ? (
                  <div className="key-saved">
                    <span>Saved key ending in <code>{hint}</code></span>
                    <button className="link" onClick={() => saveKey('')}>Remove</button>
                  </div>
                ) : (
                  <form
                    className="key-row"
                    onSubmit={e => {
                      e.preventDefault()
                      if (key.trim()) saveKey(key)
                    }}
                  >
                    <input id="llm-key" type="password" value={key} placeholder="Paste your API key" autoComplete="off" spellCheck={false} onChange={e => setKey(e.target.value)} />
                    <button type="submit" disabled={!key.trim()}>Save</button>
                  </form>
                )}
              </div>
            )}

            <ModelPicker
              key={slot}
              provider={s.provider}
              value={effectiveModel(s.provider, model)}
              listed={listed}
              listing={listing}
              listError={listError}
              waiting={isApi && !canList ? (needsKey && !hint ? 'Add your API key to see the models' : 'Add the API address to see the models') : null}
              onReload={isApi && canList ? () => setReload(n => n + 1) : null}
              onPick={id => {
                setModel(id)
                saveModel(id)
              }}
            />

            {isApi && <p className="modal-note">API calls are billed to your API account. Translating a long paper takes dozens of calls.</p>}

            <div className="test-row">
              <button className="primary" onClick={runTest} disabled={test.state === 'running' || !!status?.problem}>
                {test.state === 'running' ? 'Testing…' : 'Test'}
              </button>
              <span className={`test-result ${test.state}`} aria-live="polite">
                {test.state === 'ok' || test.state === 'error' ? test.text : status?.problem ? <ProblemLine status={status} /> : 'Ready'}
              </span>
            </div>
          </>
        )}
        {error && <div className="modal-error" role="alert">{error}</div>}
      </div>
    </div>
  )
}

const FILTER_FROM = 8

/** The models to pick from as a list. "Other model" takes any name the list doesn't have. */
function ModelPicker({
  provider,
  value,
  listed,
  listing,
  listError,
  waiting,
  onReload,
  onPick,
}: {
  provider: Provider
  value: string
  listed: ModelOption[]
  listing: boolean
  listError: string | null
  waiting: string | null
  onReload: (() => void) | null
  onPick: (id: string) => void
}) {
  const [filter, setFilter] = useState('')
  const [typing, setTyping] = useState(false)
  const [typed, setTyped] = useState('')

  const seen = new Set<string>()
  const options: ModelOption[] = []
  for (const m of [...(KNOWN[provider] ?? []), ...listed]) {
    if (seen.has(m.id)) continue
    seen.add(m.id)
    options.push(m)
  }
  // A name typed earlier that no list has stays visible and picked.
  if (value && !seen.has(value)) options.unshift({ id: value, note: 'Typed name' })
  const q = filter.trim().toLowerCase()
  const shown = q ? options.filter(m => m.id.toLowerCase().includes(q) || m.name?.toLowerCase().includes(q)) : options

  const commit = () => {
    const name = typed.trim()
    setTyping(false)
    if (name) onPick(name)
  }

  return (
    <div className="field">
      <div className="field-head">
        <span className="field-label" id="llm-model">Model</span>
        {listing ? <span className="field-aside">Loading models…</span> : onReload ? <button className="link" onClick={onReload}>Refresh list</button> : null}
      </div>
      {options.length > FILTER_FROM && (
        <input className="model-filter" value={filter} placeholder={`Filter ${options.length} models`} spellCheck={false} onChange={e => setFilter(e.target.value)} aria-label="Filter models" />
      )}
      <div className="model-list" role="radiogroup" aria-labelledby="llm-model">
        {shown.map(m => {
          const on = !typing && m.id === value
          return (
            <button key={m.id || 'default'} role="radio" aria-checked={on} className={on ? 'on' : ''} onClick={() => !on && onPick(m.id)}>
              <span className="model-main">
                <b>{m.name ?? m.id}</b>
                {m.name && m.id && <code>{m.id}</code>}
              </span>
              {m.note && <span className="model-note">{m.note}</span>}
              {on && (
                <svg className="model-check" width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>
              )}
            </button>
          )
        })}
        {q && !shown.length && <div className="model-empty">No model matches “{filter.trim()}”</div>}
        {typing ? (
          <form
            className="model-other key-row"
            onSubmit={e => {
              e.preventDefault()
              commit()
            }}
          >
            <input
              autoFocus
              value={typed}
              placeholder="Model name"
              spellCheck={false}
              onChange={e => setTyped(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Escape') {
                  e.stopPropagation()
                  setTyping(false)
                }
              }}
              aria-label="Model name"
            />
            <button type="submit" disabled={!typed.trim()}>Use</button>
          </form>
        ) : (
          <button className="model-add" onClick={() => {
            setTyped(filter.trim())
            setTyping(true)
          }}>
            Other model…
          </button>
        )}
      </div>
      {waiting && <p className="field-hint">{waiting}</p>}
      {listError && <p className="field-hint error">{listError}</p>}
    </div>
  )
}

function ProblemLine({ status }: { status: LlmStatus }) {
  switch (status.problem) {
    case 'missing':
      return <>{status.label} isn't installed</>
    case 'logged-out':
      return <>{status.label} isn't logged in</>
    case 'no-key':
      return <>Add an API key</>
    case 'no-model':
      return <>Pick a model</>
    case 'bad-url':
      return <>Add the API address (https://)</>
    default:
      return null
  }
}
