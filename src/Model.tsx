import { useEffect, useRef, useState } from 'react'
import { api, errorText, slotOf, type LlmConfig, type LlmSettings, type LlmStatus, type Provider } from './api'
import { isMac } from './platform'

const PROVIDERS: { id: Provider; name: string; sub: string }[] = [
  { id: 'claude-code', name: 'Claude Code', sub: 'Your Claude Code login' },
  { id: 'codex', name: 'Codex', sub: 'Your ChatGPT login in Codex' },
  { id: 'anthropic', name: 'Anthropic API', sub: 'Your Anthropic API key' },
  { id: 'openai', name: 'OpenAI-compatible', sub: 'OpenAI, OpenRouter, DeepSeek, Gemini, Ollama, or any URL' },
]

/** Suggestions for the model field; any other name can be typed. */
const SUGGESTED: Partial<Record<Provider, string[]>> = {
  'claude-code': ['sonnet', 'opus', 'haiku'],
  anthropic: ['claude-sonnet-5-5', 'claude-opus-5-5', 'claude-haiku-5-5'],
}

const DEFAULT_MODEL: Partial<Record<Provider, string>> = {
  'claude-code': 'sonnet',
  anthropic: 'claude-sonnet-5-5',
}

function ChipIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden="true">
      <rect x="4" y="4" width="8" height="8" rx="1.5" />
      <path d="M6.5 1.8V4M9.5 1.8V4M6.5 12v2.2M9.5 12v2.2M1.8 6.5H4M1.8 9.5H4M12 6.5h2.2M12 9.5h2.2" strokeLinecap="round" />
    </svg>
  )
}

/** Sidebar footer button naming the current model backend; opens Model settings. */
export function ModelButton({ status, onOpen }: { status: LlmStatus | null; onOpen: () => void }) {
  const label = status?.label ?? 'Model'
  return (
    <button className={`lang-btn model-btn${status?.problem ? ' warn' : ''}`} onClick={onOpen} title={`Model: ${label}. Click to change`}>
      <ChipIcon />
      <span>{label}</span>
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
  const [models, setModels] = useState<string[]>([])
  const [loadingModels, setLoadingModels] = useState(false)
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
      if (e.key === 'Escape' && !e.isComposing) {
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

  const save = (patch: Partial<LlmSettings>) => {
    const base = latest.current
    if (!base) return
    const next = { ...base, ...patch }
    latest.current = next
    setError(null)
    setModels([])
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
  // Fields save on blur, which doesn't happen when the dialog closes; save what was typed first.
  const close = () => {
    saveModel(model)
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
  const loadModels = () => {
    setLoadingModels(true)
    setError(null)
    api
      .llmModels()
      .then(list => {
        setModels(list)
        if (!list.length) setError('The API listed no models. Type the model name instead')
      })
      .catch(e => setError(errorText(e)))
      .finally(() => setLoadingModels(false))
  }
  const runTest = () => {
    setTest({ state: 'running' })
    api
      .llmTest()
      .then(r => setTest({ state: 'ok', text: `Works. Replied in ${(r.ms / 1000).toFixed(1)} s` }))
      .catch(e => setTest({ state: 'error', text: errorText(e) }))
  }

  const suggestions = [...(SUGGESTED[s?.provider ?? 'claude-code'] ?? []), ...models]
  const placeholder = s?.provider === 'codex' ? 'Account default' : DEFAULT_MODEL[s?.provider ?? 'claude-code'] ?? 'Model name, e.g. from Load models'

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
                  <b>{p.name}</b>
                  <span>{p.sub}</span>
                </button>
              ))}
            </div>

            {s.provider === 'openai' && (
              <div className="field">
                <label htmlFor="llm-preset">Service</label>
                <select id="llm-preset" value={s.preset} onChange={e => save({ preset: e.target.value })}>
                  {config.presets.map(p => (
                    <option key={p.id} value={p.id}>{p.name}</option>
                  ))}
                </select>
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

            <div className="field">
              <label htmlFor="llm-model">Model</label>
              <div className="key-row">
                <input
                  id="llm-model"
                  list="llm-models"
                  value={model}
                  placeholder={placeholder}
                  spellCheck={false}
                  onChange={e => setModel(e.target.value)}
                  onBlur={() => saveModel(model)}
                  onKeyDown={e => e.key === 'Enter' && !e.nativeEvent.isComposing && (e.target as HTMLInputElement).blur()}
                />
                {isApi && (
                  <button type="button" onClick={loadModels} disabled={loadingModels || (needsKey && !hint)}>
                    {loadingModels ? 'Loading…' : 'Load models'}
                  </button>
                )}
              </div>
              <datalist id="llm-models">
                {[...new Set(suggestions)].map(m => <option key={m} value={m} />)}
              </datalist>
            </div>

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
