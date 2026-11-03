#!/usr/bin/env node
// Checks that every t('…') / tx('…') key in src/ is in src/locales/.keys.json, then that every
// locale file has every key with the same {placeholders} (scripts/check-locales.py).
const ts = require('typescript')
const fs = require('fs')
const path = require('path')
const { execFileSync } = require('child_process')

const root = path.resolve(__dirname, '..')
const dir = path.join(root, 'src')
const known = JSON.parse(fs.readFileSync(path.join(dir, 'locales', '.keys.json'), 'utf8'))
const missing = []
for (const f of fs.readdirSync(dir).filter(f => /\.tsx?$/.test(f) && f !== 'i18n.tsx')) {
  const src = ts.createSourceFile(f, fs.readFileSync(path.join(dir, f), 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  const walk = n => {
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression) && (n.expression.text === 't' || n.expression.text === 'tx')) {
      const a = n.arguments[0]
      if (a && (ts.isStringLiteral(a) || ts.isNoSubstitutionTemplateLiteral(a)) && !(a.text in known)) {
        missing.push(`${f}:${src.getLineAndCharacterOfPosition(a.getStart()).line + 1}: ${a.text}`)
      }
    }
    ts.forEachChild(n, walk)
  }
  walk(src)
}
if (missing.length) {
  console.error('UI text missing from src/locales/.keys.json (add it there and to every locale):')
  for (const m of missing) console.error('  ' + m)
  process.exit(1)
}
execFileSync('python3', [path.join(__dirname, 'check-locales.py')], { stdio: 'inherit' })
