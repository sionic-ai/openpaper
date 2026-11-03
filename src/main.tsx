import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import 'katex/dist/katex.min.css'
import './style.css'
import { App } from './App'
import { applyTheme, initialTheme } from './theme'
import { isMac } from './platform'
import { setUiLang } from './i18n'

// Before the first paint, so a dark-mode reader never sees a white flash.
applyTheme(initialTheme())
// macOS draws the window buttons over the app; style.css leaves room for them only there.
document.documentElement.classList.toggle('mac', isMac)
// The interface follows the translation language picked in the sidebar (Korean by default).
setUiLang(localStorage.getItem('openpaper.lang') ?? 'ko')

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
)
