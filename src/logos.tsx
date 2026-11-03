import claude from './logos/claude.svg'
import codex from './logos/codex.svg'
import deepseek from './logos/deepseek.svg'
import gemini from './logos/gemini.svg'
import anthropic from './logos/anthropic.svg?raw'
import openai from './logos/openai.svg?raw'
import openrouter from './logos/openrouter.svg?raw'
import opengateway from './logos/opengateway.svg?raw'

// Color marks load as images, so their gradient ids never clash when a mark shows twice.
// One-color marks are inlined so they take the text color in light and dark themes.
const COLOR: Record<string, string> = { claude, codex, deepseek, gemini }
// Their <title> would show as a tooltip and read as text; the button already names the service.
const bare = (svg: string) => svg.replace(/<title>[^<]*<\/title>/, '')
const MONO: Record<string, string> = {
  anthropic: bare(anthropic),
  openai: bare(openai),
  openrouter: bare(openrouter),
  opengateway: bare(opengateway),
}

export type LogoId =
  | 'claude'
  | 'codex'
  | 'anthropic'
  | 'openai'
  | 'openrouter'
  | 'opengateway'
  | 'deepseek'
  | 'gemini'
  | 'ollama'
  | 'custom'

/** The provider's mark, or a plain glyph for a local server or a custom address. */
export function Logo({ id, size = 18 }: { id: LogoId; size?: number }) {
  const style = { width: size, height: size }
  const src = COLOR[id]
  if (src) return <img className="logo" src={src} alt="" style={style} draggable={false} />
  const svg = MONO[id]
  // OpenGateway's mark is wide and takes its brand blue (set in CSS).
  if (svg) return <span className={`logo logo-${id}`} style={style} aria-hidden="true" dangerouslySetInnerHTML={{ __html: svg }} />
  return (
    <span className="logo" style={style} aria-hidden="true">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
        {id === 'ollama' ? (
          <>
            <rect x="2" y="2.5" width="12" height="8.5" rx="1.5" />
            <path d="M5.5 13.5h5M8 11v2.5" />
          </>
        ) : (
          <path d="M6.8 9.2a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.9.9M9.2 6.8a2.6 2.6 0 0 0-3.7 0L3.3 9a2.6 2.6 0 0 0 3.7 3.7l.9-.9" />
        )}
      </svg>
    </span>
  )
}
