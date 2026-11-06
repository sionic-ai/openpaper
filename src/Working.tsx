import { t, useT } from './i18n'

/** Papers being fetched or translated in a background tab, by arXiv ID. */
export type Working = Record<string, { done: number; total: number }>

/** The label of a paper's button while it is fetched or translated in its own tab: a turning ring and Translating…. */
export function WorkingLabel({ job }: { job: { done: number; total: number } }) {
  useT()
  return (
    <span className="working">
      <span className="working-spin" aria-hidden="true" />
      {t('Translating…')}
      {job.total > 0 && <span className="working-n">{job.done}/{job.total}</span>}
    </span>
  )
}
