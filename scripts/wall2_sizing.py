"""Slice 3 sizing, tightened.

The first pass bucketed by 'same punctuation shape', which counts `string` vs
`number` as a RENAME. A name difference must be a difference between NAMES, so:
  - QUALIFIER: identical after stripping dotted prefixes;
  - RENAME:    same shape, same identifier count, and every differing pair is
               between two NON-KEYWORD identifiers (a keyword on either side
               means the types genuinely differ, not their spelling).
Everything else is structural or a genuine type difference.
"""
import re, collections

PATH = '/Users/mohebifar/dev/codemod/tsr/.claude/worktrees/checker-1-printing/target/verdict_baseline.tsv'
IDENT = re.compile(r'[A-Za-z_$][A-Za-z0-9_$]*')

KEYWORDS = {
    'string','number','boolean','any','void','never','unknown','undefined','null',
    'object','symbol','bigint','true','false','this','readonly','new','typeof',
    'keyof','infer','extends','is','asserts','import','error','abstract','out','in',
    'const','unique',
}

def rhs(s):
    i = s.rfind(' : ')
    return s[i+3:] if i >= 0 else s

def strip_quals(t):
    return re.sub(r'(?:[A-Za-z_$][A-Za-z0-9_$]*\.)+([A-Za-z_$])', r'\1', t)

def shape(t):
    return IDENT.sub('@', t)

def classify(v, w, g):
    if v == 'GAP':
        return 'gap'
    if w == g:
        return 'equal-rhs'
    if strip_quals(w) == strip_quals(g):
        return 'QUALIFIER'
    if shape(w) != shape(g):
        return 'structural'
    wi, gi = IDENT.findall(w), IDENT.findall(g)
    if len(wi) != len(gi):
        return 'structural'
    diffs = [(a, b) for a, b in zip(wi, gi) if a != b]
    if not diffs:
        return 'structural'
    if all(a not in KEYWORDS and b not in KEYWORDS for a, b in diffs):
        return 'RENAME'
    return 'type-differs'

rows = collections.defaultdict(list)
for line in open(PATH):
    p = line.rstrip('\n').split('\t')
    if len(p) < 4:
        continue
    rows[p[0].rsplit(':', 2)[0]].append((p[1], rhs(p[2]), rhs(p[3])))

bucket = collections.Counter()
case_bucket = collections.Counter()
conv = []
pairs = collections.Counter()

for case, lines in rows.items():
    bad = [(v, w, g) for (v, w, g) in lines if v != 'RIGHT']
    if not bad:
        continue
    kinds = set()
    for v, w, g in bad:
        k = classify(v, w, g)
        bucket[k] += 1
        kinds.add(k)
        if k == 'RENAME':
            wi, gi = IDENT.findall(w), IDENT.findall(g)
            for a, b in zip(wi, gi):
                if a != b:
                    pairs[f'{a}  <-  {b}'] += 1
    if kinds <= {'QUALIFIER', 'RENAME'}:
        conv.append((case, len(bad)))
    case_bucket['name-shaped ONLY' if kinds <= {'QUALIFIER', 'RENAME'} else 'other'] += 1

tot = sum(bucket.values())
print('=== LINE buckets (tightened) ===')
for k, v in bucket.most_common():
    print(f'{v:>8}  {100*v/tot:5.1f}%  {k}')
print(f'{tot:>8}  total non-right')

print('\n=== CASE buckets ===')
for k, v in case_bucket.most_common():
    print(f'{v:>8}  {k}')

print(f'\n=== SLICE 3 FORECAST: {len(conv)} cases, {sum(n for _, n in conv)} lines ===')
conv.sort(key=lambda x: -x[1])
for c, n in conv[:15]:
    print(f'  {n:>3}  {c}')

print('\n=== top RENAME pairs (want <- got) ===')
for k, v in pairs.most_common(20):
    print(f'  {v:>5}  {k}')
