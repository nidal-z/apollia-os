#!/usr/bin/env python3
"""Vérifie les includes cassés dans docs/book/src/."""
import os
import glob
import re
import sys

errors = 0
for md in glob.glob('docs/book/src/**/*.md', recursive=True):
    d = os.path.dirname(md)
    for line in open(md):
        m = re.search(r'#include\s+([^\s}]+)', line)
        if m:
            target = os.path.normpath(os.path.join(d, m.group(1)))
            if not os.path.isfile(target):
                print(f'❌ {md} → {m.group(1)}')
                errors += 1

if errors == 0:
    print('✅ Tous les includes sont valides')
else:
    print(f'❌ {errors} include(s) cassé(s)')
    sys.exit(1)
