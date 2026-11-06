import type { Roadmap, RoadmapItem, RoadmapRelation, RoadmapStep } from './api'
import { t, useT } from './i18n'
import { WorkingLabel, type Working } from './Working'

export type RoadmapState = {
  /** Library id of the seed paper. */
  docId: string
  seedTitle: string
  data: Roadmap | null
  step: RoadmapStep | null
  error: string | null
}

type Props = {
  state: RoadmapState
  /** arXiv IDs already in the library. */
  saved: Set<string>
  working: Working
  onOpen: (item: RoadmapItem) => void
  onWeb: (id: string) => void
  onRegenerate: () => void
  onBack: () => void
}

const STEPS: RoadmapStep[] = ['references', 'citing', 'ordering']

function stepText(step: RoadmapStep): string {
  switch (step) {
    case 'references':
      return t('Finding the papers it builds on')
    case 'citing':
      return t('Finding the papers that build on it')
    case 'ordering':
      return t('Putting them in reading order')
  }
}

function relationText(r: RoadmapRelation): string {
  switch (r) {
    case 'key-reference':
      return t('Key reference')
    case 'reference':
      return t('Cited by this paper')
    case 'citing':
      return t('Cites this paper')
    case 'recent-citing':
      return t('Recent follow-up')
  }
}

function count(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n)
}

/** Reading roadmap around one paper: stages before it, the paper itself, stages after it. */
export function RoadmapView({ state, saved, working, onOpen, onWeb, onRegenerate, onBack }: Props) {
  useT()
  const { data, step, error, seedTitle } = state
  const before = data?.stages.filter(s => !s.after) ?? []
  const after = data?.stages.filter(s => s.after) ?? []
  let n = 0

  const card = (item: RoadmapItem) => {
    const have = saved.has(item.id)
    return (
      <li key={item.id} className="rm-card">
        <div className="rm-card-head">
          <b lang="en">{item.title}</b>
          <span className="rm-meta">
            {[item.year, item.citations != null ? t('{n} citations', { n: count(item.citations) }) : null, relationText(item.relation)]
              .filter(Boolean)
              .join(' · ')}
          </span>
        </div>
        {item.why && <p className="rm-why">{item.why}</p>}
        {item.focus && (
          <p className="rm-focus">
            <span>{t('Look at')}</span> {item.focus}
          </p>
        )}
        <div className="rm-actions">
          <button className={have ? '' : 'primary'} onClick={() => onOpen(item)} title={working[item.id] ? t('Translating in a new tab. Click to go to it') : undefined}>
            {working[item.id] ? <WorkingLabel job={working[item.id]!} /> : have ? t('Open from library') : t('Translate')}
          </button>
          <button className="bar-link" onClick={() => onWeb(item.id)} title={t('Open arxiv.org in your browser')}>
            arxiv.org ↗
          </button>
          {have && <span className="rm-have">{t('In your library')}</span>}
        </div>
      </li>
    )
  }

  const stage = (s: Roadmap['stages'][number], i: number) => {
    n += 1
    return (
      <section key={`${s.after}-${i}`} className="rm-stage">
        <div className="rm-stage-head">
          <span className="rm-num" aria-hidden="true">{n}</span>
          <div>
            <h2>{s.name}</h2>
            {s.goal && <p>{s.goal}</p>}
          </div>
        </div>
        <ul>{s.items.map(card)}</ul>
      </section>
    )
  }

  return (
    <section className="roadmap">
      <div className="rm-top">
        <div>
          <h1>{data?.title || t('Reading roadmap')}</h1>
          <p className="rm-seed">
            {t('Starts from')} <b lang="en">{seedTitle}</b>
          </p>
        </div>
        <div className="rm-top-actions">
          <button onClick={onBack}>{t('Back to the paper')}</button>
          {data && (
            <button onClick={onRegenerate} disabled={!!step}>
              {t('Make it again')}
            </button>
          )}
        </div>
      </div>

      {step && (
        <ol className="rm-progress" aria-live="polite">
          {STEPS.map(s => {
            const at = STEPS.indexOf(step)
            const i = STEPS.indexOf(s)
            return (
              <li key={s} className={i < at ? 'done' : i === at ? 'now' : ''}>
                {stepText(s)}
              </li>
            )
          })}
        </ol>
      )}
      {error && <div className="error" role="alert">{error}</div>}

      {data && !step && (
        <div className="rm-path">
          {before.map(stage)}
          <section className="rm-stage rm-seed-stage">
            <div className="rm-stage-head">
              <span className="rm-num" aria-hidden="true">{++n}</span>
              <div>
                <h2>{t('This paper')}</h2>
                <p lang="en">{seedTitle}</p>
              </div>
            </div>
          </section>
          {after.map(stage)}
          {!data.openalex && <p className="rm-note">{t("Citation data wasn't available, so this roadmap uses only the paper's own references.")}</p>}
        </div>
      )}
    </section>
  )
}
