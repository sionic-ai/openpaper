import { useEffect, useRef, useState } from 'react'
import { api, errorText, slotOf, type LlmBackup, type LlmConfig, type LlmSettings, type LlmStatus, type ModelOption, type Preset, type Provider } from './api'
import { Logo, type LogoId } from './logos'
import { isMac } from './platform'
import { tError, t, tx, useT } from './i18n'

const PROVIDERS: { id: Provider; name: string; sub: string; logo: LogoId }[] = [
  { id: 'claude-code', name: 'Claude Code', sub: 'Your Claude Code login', logo: 'claude' },
  { id: 'codex', name: 'Codex', sub: 'Your ChatGPT login in Codex', logo: 'codex' },
  { id: 'anthropic', name: 'Anthropic API', sub: 'Your Anthropic API key', logo: 'anthropic' },
  { id: 'openai', name: 'OpenAI-compatible', sub: 'OpenAI, OpenRouter, DeepSeek, Gemini, Ollama, or any URL', logo: 'openai' },
]

const OPENGATEWAY = 'opengateway'

const PRESET_LOGO: Record<string, LogoId> = {
  opengateway: 'opengateway',
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

/** Key and model slot, as in Rust: `claude-code`, `codex`, `anthropic` or `openai:<preset>`. */
function slotKey(provider: Provider, preset: string): string {
  return provider === 'openai' ? `openai:${preset}` : provider
}

/** Models offered before any list loads, by slot. The first one is the default when none is picked. */
const KNOWN: Record<string, ModelOption[]> = {
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
  'openai:opengateway': [
    { id: 'anthropic/claude-sonnet-5-5', name: 'Claude Sonnet 5.5', note: 'Balanced. Good for most papers' },
    { id: 'openai/gpt-5.5', name: 'GPT-5.5', note: 'OpenAI. Costs more per paper' },
    { id: 'google/gemini-3.5-flash', name: 'Gemini 3.5 Flash', note: 'Fast and lower cost' },
    { id: 'deepseek/deepseek-v4.1-flash-ultrafast', name: 'DeepSeek V4.1 Flash Ultrafast', note: 'Fastest and cheapest' },
  ],
}

/** The model in effect: the saved one, or the default for backends that have one. */
function effectiveModel(slot: string, saved: string): string {
  return saved || (slot === 'codex' ? '' : KNOWN[slot]?.[0]?.id ?? '')
}

/** Short name of a model for the sidebar and the dialog header. */
function modelName(m: { provider: Provider; preset: string; model: string }): string {
  const known = KNOWN[slotKey(m.provider, m.preset)]?.find(k => k.id === m.model)
  if (known?.name) return t(known.name)
  return m.model || (m.provider === 'codex' ? t('Codex default') : t('No model picked'))
}

/** Sidebar footer button showing the mark and model answering calls; opens Model settings. */
export function ModelButton({ status, onOpen }: { status: LlmStatus | null; onOpen: () => void }) {
  useT()
  const backup = status?.backup
  const shown = backup ?? status
  const text = shown ? modelName(shown) : t('Model')
  const title = !status
    ? t('Model settings')
    : backup
      ? t('{reason}. Using the backup {label}, {model}, for now. Click to change', { reason: tError(backup.reason), label: t(backup.label), model: text })
      : t('Model: {label}, {model}. Click to change', { label: status.label, model: text })
  const warn = status?.problem && !backup
  return (
    <button className={`lang-btn model-btn${warn ? ' warn' : ''}${backup ? ' on-backup' : ''}`} onClick={onOpen} title={title}>
      {shown ? <Logo id={logoOf(shown.provider, shown.preset)} size={14} /> : null}
      <span>{text}</span>
    </button>
  )
}

/** One line for the notice above the reader when the chosen backend can't run. */
export function ProblemNotice({ status, onOpen }: { status: LlmStatus; onOpen: () => void }) {
  useT()
  const shell = isMac ? 'Terminal' : 'PowerShell'
  const open = (
    <button className="notice-link" onClick={onOpen}>
      {t('Model settings')}
    </button>
  )
  const { provider, problem, label } = status
  let text: React.ReactNode = null
  if (provider === 'claude-code' && problem === 'missing') {
    text = isMac
      ? tx("Claude Code isn't installed, so translation and AI search are off. In Terminal, run {install}, then {login}. Or pick another model in {open}.", {
          install: <code>curl -fsSL https://claude.ai/install.sh | bash</code>,
          login: <code>claude auth login</code>,
          open,
        })
      : tx("Claude Code isn't installed, so translation and AI search are off. In PowerShell, run {install}, then {login}. Or pick another model in {open}.", {
          install: <code>irm https://claude.ai/install.ps1 | iex</code>,
          login: <code>claude auth login</code>,
          open,
        })
  } else if (provider === 'claude-code' && problem === 'logged-out') {
    text = tx("Claude Code isn't logged in. Run {login} in {shell}. The app checks again when you come back to this window.", { login: <code>claude auth login</code>, shell })
  } else if (provider === 'codex' && problem === 'missing') {
    text = tx("Codex isn't installed, so translation and AI search are off. Install the Codex CLI or the ChatGPT app, or pick another model in {open}.", { open })
  } else if (provider === 'codex' && problem === 'logged-out') {
    text = tx("Codex isn't logged in. Run {login} in {shell}. The app checks again when you come back to this window.", { login: <code>codex login</code>, shell })
  } else if (problem === 'no-key') {
    text = tx('{label} needs an API key. Add it in {open}.', { label, open })
  } else if (problem === 'no-model') {
    text = tx('Pick a {label} model in {open}.', { label, open })
  } else if (problem === 'bad-url') {
    text = tx('Add the API address in {open}. It must start with https://.', { open })
  }
  return text ? <div className="notice claude-notice" role="status">{text}</div> : null
}

type Test = { state: 'idle' } | { state: 'running' } | { state: 'ok'; text: string } | { state: 'error'; text: string }

/** Model settings: which backend, its key and model, and a test call. Every change saves at once. */
export function ModelDialog({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  useT()
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
  // OpenGateway is an OpenAI-compatible service, but the dialog offers it as its own choice.
  const gateway = s?.provider === 'openai' && s.preset === OPENGATEWAY
  const isApi = s?.provider === 'anthropic' || s?.provider === 'openai'
  const needsKey = s?.provider === 'anthropic' || (s?.provider === 'openai' && (preset?.needsKey ?? true))
  const hint = config?.keyHints[slot]
  // Every service that takes a key has a page to create one, except a custom address.
  const keyPage = s?.provider === 'anthropic' || (s?.provider === 'openai' && s.preset !== 'custom')

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
      .then(r => setTest({ state: 'ok', text: t('Works. Replied in {s} s', { s: (r.ms / 1000).toFixed(1) }) }))
      .catch(e => setTest({ state: 'error', text: errorText(e) }))
  }

  return (
    <div className="modal-back" onMouseDown={e => e.target === e.currentTarget && close()}>
      <div className="modal model-dialog" role="dialog" aria-modal="true" aria-labelledby="model-title" ref={box} tabIndex={-1}>
        <div className="modal-head">
          <h2 id="model-title">{t('Model')}</h2>
          <button className="modal-close" onClick={close} aria-label={t('Close')}>
            <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true"><path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>
          </button>
        </div>
        <p className="modal-sub">{t('What runs translation, summaries, questions and the AI finder.')}</p>
        {status && <NowUsing status={status} />}
        {s && (
          <>
            <div className="providers" role="radiogroup" aria-label={t('Model backend')}>
              {PROVIDERS.map(p => (
                <button
                  key={p.id}
                  role="radio"
                  aria-checked={s.provider === p.id && !gateway}
                  className={s.provider === p.id && !gateway ? 'on' : ''}
                  onClick={() => {
                    if (p.id === 'openai' && gateway) save({ preset: 'openai' })
                    else if (s.provider !== p.id) save({ provider: p.id })
                  }}
                >
                  <span className="provider-name">
                    <Logo id={p.logo} size={18} />
                    <b>{t(p.name)}</b>
                  </span>
                  <span className="provider-sub">{t(p.sub)}</span>
                </button>
              ))}
              <button
                role="radio"
                aria-checked={gateway}
                className={`og-card${gateway ? ' on' : ''}`}
                onClick={() => !gateway && save({ provider: 'openai', preset: OPENGATEWAY })}
              >
                <Logo id="opengateway" size={30} />
                <span className="og-text">
                  <b>OpenGateway</b>
                  <span>{t('One key for Claude, GPT, Gemini and more')}</span>
                </span>
                <span className="og-site">opengateway.ai</span>
              </button>
            </div>

            {s.provider === 'openai' && !gateway && (
              <div className="field">
                <span className="field-label" id="llm-preset">{t('Service')}</span>
                <div className="presets" role="radiogroup" aria-labelledby="llm-preset">
                  {config.presets.filter(p => p.id !== OPENGATEWAY).map(p => (
                    <button
                      key={p.id}
                      role="radio"
                      aria-checked={s.preset === p.id}
                      className={s.preset === p.id ? 'on' : ''}
                      onClick={() => s.preset !== p.id && save({ preset: p.id })}
                    >
                      <Logo id={PRESET_LOGO[p.id] ?? 'custom'} size={16} />
                      <span>{t(p.name)}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
            {s.provider === 'openai' && s.preset === 'custom' && (
              <div className="field">
                <label htmlFor="llm-base">{t('API address')}</label>
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
                <div className="field-head">
                  <label htmlFor="llm-key">{t('API key')}</label>
                  {!hint && keyPage && (
                    <button className="link" onClick={() => void api.llmOpenKeyPage().catch(e => setError(errorText(e)))}>
                      {gateway ? t('Get a key at opengateway.ai') : t('Get a key')}
                    </button>
                  )}
                </div>
                {hint ? (
                  <div className="key-saved">
                    <span>{tx('Saved key ending in {hint}', { hint: <code>{hint}</code> })}</span>
                    <button className="link" onClick={() => saveKey('')}>{t('Remove')}</button>
                  </div>
                ) : (
                  <form
                    className="key-row"
                    onSubmit={e => {
                      e.preventDefault()
                      if (key.trim()) saveKey(key)
                    }}
                  >
                    <input id="llm-key" type="password" value={key} placeholder={t('Paste your API key')} autoComplete="off" spellCheck={false} onChange={e => setKey(e.target.value)} />
                    <button type="submit" disabled={!key.trim()}>{t('Save')}</button>
                  </form>
                )}
              </div>
            )}

            <ModelPicker
              key={slot}
              known={KNOWN[slot] ?? []}
              value={effectiveModel(slot, model)}
              listed={listed}
              listing={listing}
              listError={listError}
              waiting={isApi && !canList ? (needsKey && !hint ? t('Add your API key to see the models') : t('Add the API address to see the models')) : null}
              onReload={isApi && canList ? () => setReload(n => n + 1) : null}
              onPick={id => {
                setModel(id)
                saveModel(id)
              }}
            />

            {isApi && <p className="modal-note">{t('API calls are billed to your API account. Translating a long paper takes dozens of calls.')}</p>}

            <BackupPicker
              settings={s}
              presets={config.presets}
              hints={config.keyHints}
              label={status?.label ?? t('The model')}
              onChange={backups => save({ backups })}
            />

            <div className="test-row">
              <button className="primary" onClick={runTest} disabled={test.state === 'running' || !!status?.problem}>
                {test.state === 'running' ? t('Testing…') : t('Test')}
              </button>
              <span className={`test-result ${test.state}`} aria-live="polite">
                {test.state === 'ok' || test.state === 'error' ? test.text : status?.problem ? <ProblemLine status={status} /> : t('Ready')}
              </span>
            </div>
          </>
        )}
        {error && <div className="modal-error" role="alert">{error}</div>}
      </div>
    </div>
  )
}

/** The backend answering calls: the chosen one, or the backup that took over and why. */
function NowUsing({ status }: { status: LlmStatus }) {
  useT()
  const b: LlmBackup | null = status.backup
  const shown = b ?? status
  return (
    <div className={`now-using${status.problem && !b ? ' warn' : ''}${b ? ' on-backup' : ''}`} aria-live="polite">
      <Logo id={logoOf(shown.provider, shown.preset)} size={20} />
      <span>
        <small>{t('Now using')}</small>
        <b>{shown.label}</b>
        <span className="sep" aria-hidden="true">·</span>
        <span className="now-model">{modelName(shown)}</span>
        {b && <span className="backup-tag">{t('Backup')}</span>}
        {b && (
          <span className="now-why">
            {t('{reason}. {label} is tried again within 20 minutes.', { reason: tError(b.reason), label: t(status.label) })}
          </span>
        )}
      </span>
    </div>
  )
}

type Candidate = { slot: string; name: string; logo: LogoId }

/**
 * Backups, in order: when the chosen backend hits a usage limit or can't run, the next one
 * answers instead. Lists every backend with a login or a saved key, except the chosen one.
 */
function BackupPicker({
  settings: s,
  presets,
  hints,
  label,
  onChange,
}: {
  settings: LlmSettings
  presets: Preset[]
  hints: Record<string, string>
  label: string
  onChange: (backups: string[]) => void
}) {
  useT()
  const chosen = slotOf(s)
  const known = (slot: string) => s.backups.includes(slot) || slot in hints || !!s.models[slot]
  const candidates: Candidate[] = [
    ...PROVIDERS.filter(p => p.id !== 'openai').map(p => ({ slot: p.id as string, name: p.name, logo: p.logo })),
    { slot: `openai:${OPENGATEWAY}`, name: 'OpenGateway', logo: 'opengateway' as LogoId },
    ...presets
      .filter(p => p.id !== OPENGATEWAY && known(`openai:${p.id}`))
      .map(p => ({ slot: `openai:${p.id}`, name: p.name, logo: PRESET_LOGO[p.id] ?? ('custom' as LogoId) })),
  ].filter(c => c.slot !== chosen)

  // What a backup still needs before it can answer, as in Rust's `backend`.
  const missing = (slot: string): string | null => {
    if (slot === 'claude-code' || slot === 'codex') return null
    const preset = presets.find(p => `openai:${p.id}` === slot)
    const needsKey = slot === 'anthropic' || (preset?.needsKey ?? false)
    if (needsKey && !(slot in hints)) return t('Needs an API key')
    if (slot === 'openai:custom' && !s.baseUrl) return t('Needs the API address')
    if (slot.startsWith('openai:') && !s.models[slot] && !preset?.model) return t('Needs a model')
    return null
  }
  const toggle = (slot: string) => onChange(s.backups.includes(slot) ? s.backups.filter(b => b !== slot) : [...s.backups, slot])

  return (
    <div className="field backups">
      <span className="field-label" id="llm-backups">{t('Backups')}</span>
      <p className="field-hint top">{t("When {label} hits a usage limit or can't run, the first backup that works takes over.", { label })}</p>
      <div className="backup-list" role="group" aria-labelledby="llm-backups">
        {candidates.map(c => {
          const at = s.backups.indexOf(c.slot)
          const on = at >= 0
          const need = missing(c.slot)
          const viaApi = c.slot.startsWith('openai:')
          const model = modelName({
            provider: viaApi ? 'openai' : (c.slot as Provider),
            preset: viaApi ? c.slot.slice('openai:'.length) : '',
            model: effectiveModel(c.slot, s.models[c.slot] ?? ''),
          })
          return (
            <button key={c.slot} role="checkbox" aria-checked={on} className={on ? 'on' : ''} disabled={!on && !!need} onClick={() => toggle(c.slot)}>
              <span className="backup-order" aria-hidden="true">{on ? at + 1 : ''}</span>
              <Logo id={c.logo} size={16} />
              <span className="backup-name">
                <b>{c.name}</b>
                <span>{need ?? model}</span>
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )
}

const FILTER_FROM = 8

/** The models to pick from as a list. "Other model" takes any name the list doesn't have. */
function ModelPicker({
  known,
  value,
  listed,
  listing,
  listError,
  waiting,
  onReload,
  onPick,
}: {
  known: ModelOption[]
  value: string
  listed: ModelOption[]
  listing: boolean
  listError: string | null
  waiting: string | null
  onReload: (() => void) | null
  onPick: (id: string) => void
}) {
  useT()
  const [filter, setFilter] = useState('')
  const [typing, setTyping] = useState(false)
  const [typed, setTyped] = useState('')

  const seen = new Set<string>()
  const options: ModelOption[] = []
  for (const m of [...known, ...listed]) {
    if (seen.has(m.id)) continue
    seen.add(m.id)
    options.push(m)
  }
  // A name typed earlier that no list has stays visible and picked.
  if (value && !seen.has(value)) options.unshift({ id: value, note: t('Typed name') })
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
        <span className="field-label" id="llm-model">{t('Model')}</span>
        {listing ? <span className="field-aside">{t('Loading models…')}</span> : onReload ? <button className="link" onClick={onReload}>{t('Refresh list')}</button> : null}
      </div>
      {options.length > FILTER_FROM && (
        <input className="model-filter" value={filter} placeholder={t('Filter {n} models', { n: options.length })} spellCheck={false} onChange={e => setFilter(e.target.value)} aria-label={t('Filter models')} />
      )}
      <div className="model-list" role="radiogroup" aria-labelledby="llm-model">
        {shown.map(m => {
          const on = !typing && m.id === value
          return (
            <button key={m.id || 'default'} role="radio" aria-checked={on} className={on ? 'on' : ''} onClick={() => !on && onPick(m.id)}>
              <span className="model-main">
                <b>{m.name ? t(m.name) : m.id}</b>
                {m.name && m.id && <code>{m.id}</code>}
              </span>
              {m.note && <span className="model-note">{t(m.note)}</span>}
              {on && (
                <svg className="model-check" width="14" height="14" viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5l3 3 6-7" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" /></svg>
              )}
            </button>
          )
        })}
        {q && !shown.length && <div className="model-empty">{t('No model matches “{q}”', { q: filter.trim() })}</div>}
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
              placeholder={t('Model name')}
              spellCheck={false}
              onChange={e => setTyped(e.target.value)}
              onKeyDown={e => {
                if (e.key === 'Escape') {
                  e.stopPropagation()
                  setTyping(false)
                }
              }}
              aria-label={t('Model name')}
            />
            <button type="submit" disabled={!typed.trim()}>{t('Use')}</button>
          </form>
        ) : (
          <button className="model-add" onClick={() => {
            setTyped(filter.trim())
            setTyping(true)
          }}>
            {t('Other model…')}
          </button>
        )}
      </div>
      {waiting && <p className="field-hint">{waiting}</p>}
      {listError && <p className="field-hint error">{listError}</p>}
    </div>
  )
}

function ProblemLine({ status }: { status: LlmStatus }) {
  useT()
  switch (status.problem) {
    case 'missing':
      return <>{t("{label} isn't installed", { label: status.label })}</>
    case 'logged-out':
      return <>{t("{label} isn't logged in", { label: status.label })}</>
    case 'no-key':
      return <>{t('Add an API key')}</>
    case 'no-model':
      return <>{t('Pick a model')}</>
    case 'bad-url':
      return <>{t('Add the API address (https://)')}</>
    default:
      return null
  }
}
