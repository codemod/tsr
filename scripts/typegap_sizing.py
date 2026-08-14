"""Where the remaining 75.8% lives: bucket the TYPE-COMPUTATION damage.

`wall2_sizing.py` established that naming is 3.1% of the corpus's remaining
damage and 79 cases. This sizes the rest — the lines where the checker computes
a DIFFERENT TYPE — by the shape of what we print against what is wanted, and
then reports the case-level conversion for each mechanism: how many failing
cases would pass if that ONE bucket were fixed.

Input is target/verdict_baseline.tsv (case:file:pos, verdict, want, got).
No checker API; run after any `scorepair --accept`.
"""
import re, collections

PATH = '/Users/mohebifar/dev/codemod/tsr/.claude/worktrees/checker-1-printing/target/verdict_baseline.tsv'

def rhs(s):
    i = s.rfind(' : ')
    return s[i+3:] if i >= 0 else s

PRIMS = {'string','number','boolean','void','undefined','null','never','unknown',
         'object','symbol','bigint','true','false','any'}

def kind(t):
    t = t.strip()
    if t == 'any':      return 'any'
    if t == 'error':    return 'error'
    if t in PRIMS:      return 'primitive'
    if re.fullmatch(r'"[^"]*"|-?\d+(\.\d+)?n?|`.*`', t): return 'literal'
    if t.startswith('typeof '):  return 'typeof'
    if ' => ' in t or t.startswith('new ('): return 'function'
    if t.startswith('{'):        return 'object'
    if t.startswith('['):        return 'tuple'
    if t.endswith('[]'):         return 'array'
    if ' | ' in t:               return 'union'
    if ' & ' in t:               return 'intersection'
    if '<' in t:                 return 'generic-ref'
    if re.fullmatch(r'[A-Za-z_$][\w$.]*', t): return 'name'
    return 'other'

rows = collections.defaultdict(list)
for line in open(PATH):
    p = line.rstrip('\n').split('\t')
    if len(p) < 4:
        continue
    rows[p[0].rsplit(':', 2)[0]].append((p[1], rhs(p[2]), rhs(p[3])))

pair = collections.Counter()
case_single = collections.Counter()
case_lines = collections.Counter()

for case, lines in rows.items():
    bad = [(v, w, g) for (v, w, g) in lines if v != 'RIGHT']
    if not bad:
        continue
    kinds = set()
    for v, w, g in bad:
        k = f'{kind(g):>11}  ->  {kind(w)}'
        pair[k] += 1
        kinds.add(k)
    if len(kinds) == 1:
        only = next(iter(kinds))
        case_single[only] += 1
        case_lines[only] += len(bad)

print('=== every non-right line, as (what we print) -> (what is wanted) ===')
tot = sum(pair.values())
for k, v in pair.most_common(24):
    print(f'{v:>7}  {100*v/tot:5.1f}%  {k}')
print(f'{tot:>7}  total\n')

print('=== CASES blocked by exactly ONE of these transitions ===')
print('    (fixing that single transition converts the whole case)\n')
print(f'{"cases":>6} {"lines":>7}   transition')
for k, v in case_single.most_common(22):
    print(f'{v:>6} {case_lines[k]:>7}   {k}')
print(f'\n{sum(case_single.values())} single-transition cases '
      f'out of {sum(1 for c, l in rows.items() if any(v != "RIGHT" for v, _, _ in l))} failing')
