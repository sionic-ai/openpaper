#!/usr/bin/env python3
"""Checks src/locales/<code>.json against the key list: every key present, placeholders kept."""
import json, re, sys, pathlib
root = pathlib.Path(__file__).resolve().parent.parent / 'src' / 'locales'
keys = json.loads((root / '.keys.json').read_text())
ph = lambda s: sorted(re.findall(r'\{\w+\}', s))
bad = 0
for f in sorted(root.glob('*.json')):
    if f.name.startswith('.') or (len(sys.argv) > 1 and f.stem not in sys.argv[1:]):
        continue
    d = json.loads(f.read_text())
    missing = [k for k in keys if not d.get(k)]
    extra = [k for k in d if k not in keys]
    wrong = [k for k in keys if d.get(k) and ph(d[k]) != ph(k)]
    bang = [k for k in d if '!' in d[k] or '！' in d[k]]
    for name, items in (('missing', missing), ('extra', extra), ('placeholder', wrong), ('exclamation', bang)):
        for k in items:
            print(f'{f.name}: {name}: {k}')
    bad += len(missing) + len(extra) + len(wrong) + len(bang)
    print(f'{f.name}: {len(d)} entries')
sys.exit(1 if bad else 0)
