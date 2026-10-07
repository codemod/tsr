import xml.etree.ElementTree as E
from collections import Counter,defaultdict
import json,re,sys,hashlib
p=sys.argv[1];root=E.parse(p).getroot(); ids={e.get('id'):e for e in root.iter() if e.get('id')}
def resolve(e):
    while e is not None and e.get('ref'):e=ids[e.get('ref')]
    return e
def name(f):return re.sub(r'::h[0-9a-f]{16}$','',f.get('name','unknown'))
flat=Counter();inc=Counter();threads={};categories=Counter();states=Counter();count=0
for row in root.iter('row'):
    b=resolve(row.find('backtrace'));w=int(resolve(row.find('weight')).text);t=resolve(row.find('thread'));ts=int(resolve(row.find('sample-time')).text);state=resolve(row.find('thread-state')).text
    states[state]+=w
    if state!='Running':continue
    names=[name(resolve(f)) for f in b] if b is not None else []
    count+=1
    if names:flat[names[0]]+=w;inc.update({n:w for n in set(names)})
    tr=threads.setdefault(t.get('fmt'),{'cpu_ns':0,'first_ns':ts,'last_ns':ts,'samples':0})
    tr['cpu_ns']+=w;tr['last_ns']=ts;tr['samples']+=1
    for cat,pattern in {
        'checker':'tsr_checker', 'parser':'tsr_parser','binder':'tsr_binder',
        'instantiate':'instantiate_type', 'mentions':'mentions_type_parameter',
        'members':'tsr_checker::members', 'effects':'get_effects_signature','alias':'resolve_alias', 'receiver_instantiation':'instantiate_for_reference_with_this', 'member_names':'collect_structured_property_names', 'printing':'type_to_string_at_worker',
        'store_intern':'tsr_checker::store', 'symbol_type':'get_type_of_symbol',
        'relation':'tsr_checker::relater','declared':'tsr_checker::declared',
        'alloc_leaf':None
    }.items():
        if (pattern is not None and any(pattern in n for n in names)) or (cat=='alloc_leaf' and names and re.search(r'malloc|free|realloc|zone_|allocate|alloc::alloc',names[0])):categories[cat]+=w
report={'xml_sha256':hashlib.sha256(open(p,'rb').read()).hexdigest(),'running_samples':count,'sample_weight_ns':states['Running'],'states_ns':dict(states),'categories_ns':dict(categories),'threads':threads,'flat':flat.most_common(70),'inclusive':inc.most_common(100)}
open(p+'.summary.json','w').write(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items() if k not in ('flat','inclusive','threads')},indent=2))
print('TOP FLAT',report['flat'][:12]);print('TOP INCLUSIVE',report['inclusive'][:20])
