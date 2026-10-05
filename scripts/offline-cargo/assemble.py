import sys, os, shutil, json, tomllib, re
import glob; sys.path[:0] = glob.glob("/root/.cache/uv/archive-v0/*/tomlkit/..")
import tomlkit
V="/tmp/claude-0/vend"; OUT=V+"/vendor"
lock=tomllib.load(open(os.environ.get("TSR_ROOT",os.getcwd())+"/Cargo.lock","rb"))
sums={(p["name"],p["version"]):p.get("checksum") for p in lock["package"]}
os.makedirs(OUT,exist_ok=True)
def ws_root(d):
    p=os.path.dirname(os.path.abspath(d))
    while p.startswith(V+"/src/"):
        f=os.path.join(p,"Cargo.toml")
        if os.path.exists(f) and "workspace" in tomllib.load(open(f,"rb")): return tomllib.load(open(f,"rb"))["workspace"]
        p=os.path.dirname(p)
    f=os.path.join(d,"Cargo.toml"); t=tomllib.load(open(f,"rb"))
    return t.get("workspace",{})
def fix_deps(tab, ws):
    for k in list(tab.keys()):
        v=tab[k]
        if isinstance(v,dict) and v.get("workspace") is True:
            base=ws.get("dependencies",{})[k]
            base=dict(base) if isinstance(base,dict) else {"version":base}
            base.pop("path",None)
            for kk,vv in v.items():
                if kk=="workspace": continue
                if kk=="features": base["features"]=list(base.get("features",[]))+list(vv)
                else: base[kk]=vv
            t=tomlkit.inline_table(); t.update(base); tab[k]=t
        elif isinstance(v,dict) and "path" in v:
            nv={kk:vv for kk,vv in v.items() if kk!="path"}
            t=tomlkit.inline_table(); t.update(nv); tab[k]=t
def mk(name, src, sub):
    d=os.path.join(V,"src",src,sub)
    doc=tomlkit.parse(open(os.path.join(d,"Cargo.toml")).read())
    ver=None
    ws=ws_root(d)
    pk=doc["package"]
    for k in list(pk.keys()):
        v=pk[k]
        if isinstance(v,dict) and v.get("workspace") is True: pk[k]=ws["package"][k]
    if "workspace" in doc: del doc["workspace"]
    if "lints" in doc and isinstance(doc["lints"],dict) and doc["lints"].get("workspace"): del doc["lints"]
    for sec in ["dependencies","dev-dependencies","build-dependencies"]:
        if sec in doc: fix_deps(doc[sec],ws)
    if "dev-dependencies" in doc: del doc["dev-dependencies"]
    for tk,tv in doc.get("target",{}).items():
        for sec in ["dependencies","dev-dependencies","build-dependencies"]:
            if sec in tv: fix_deps(tv[sec],ws)
        if "dev-dependencies" in tv: del tv["dev-dependencies"]
    for sec in ["bench","test","example"]:
        if sec in doc: del doc[sec]
    deps=set()
    for sec in ["dependencies","build-dependencies"]:
        deps|=set(doc.get(sec,{}).keys())
    for tk,tv in doc.get("target",{}).items():
        for sec in ["dependencies","build-dependencies"]: deps|=set(tv.get(sec,{}).keys())
    feats=doc.get("features",{})
    def ok(x):
        x=str(x)
        if x.startswith("dep:"): return x[4:] in deps
        if "/" in x: return x.split("/")[0].rstrip("?") in deps
        return True
    for f in list(feats.keys()):
        feats[f]=[x for x in feats[f] if ok(x)]
    ver=str(pk["version"])
    o=os.path.join(OUT,f"{name}-{ver}")
    if os.path.exists(o): shutil.rmtree(o)
    shutil.copytree(d,o,ignore=shutil.ignore_patterns(".git"))
    open(os.path.join(o,"Cargo.toml"),"w").write(tomlkit.dumps(doc))
    json.dump({"files":{},"package":sums.get((name,ver))},open(os.path.join(o,".cargo-checksum.json"),"w"))
    print(name,ver,"ok" if sums.get((name,ver)) else "NOT IN LOCK")
for line in open(V+"/list.txt"):
    c,r,t,s=line.split()
    src=r.replace("/","_")+"@"+t
    if not os.path.isdir(os.path.join(V,"src",src)):
        src=r.replace("/","_")+"@"+t.lstrip("v")
    mk(c,src,s)
for name,ver,feats in [("windows-sys","0.61.2",None),("windows-link","0.2.1",None)]:
    o=os.path.join(OUT,f"{name}-{ver}"); os.makedirs(o+"/src",exist_ok=True)
    fl=["Win32_System_Memory","Win32_Foundation","Win32_System_Threading","Win32_System_SystemInformation","Win32_System_Diagnostics_Debug","Win32_System_LibraryLoader","Win32_Storage_FileSystem","Win32_System_Com","Win32_Security","Win32_System_IO","Win32_System_Pipes","Win32_System_Registry","Win32","Win32_System","Win32_System_Diagnostics","Win32_Storage","default"]
    feats="\n".join(f'{f} = []' for f in fl)
    open(o+"/Cargo.toml","w").write(f'[package]\nname="{name}"\nversion="{ver}"\nedition="2021"\n'+('[dependencies]\nwindows-link="0.2.1"\n' if name=="windows-sys" else '')+f'[features]\n{feats}\n')
    open(o+"/src/lib.rs","w").write("")
    json.dump({"files":{},"package":sums.get((name,ver))},open(o+"/.cargo-checksum.json","w"))
