import json, sys, collections
raw = open(sys.argv[1], encoding='utf-8').read()
start = raw.find('['); log = json.loads(raw[start:raw.rfind(']')+1])
outs = [e for e in log if e['dir']=='OUT']
ins  = [e for e in log if e['dir']=='IN']
b = lambda e: bytes.fromhex(e['data'])
# group OUT commands by first 2 bytes
cmds = collections.OrderedDict()
for e in outs:
    d=b(e); k=f"{d[0]:02x} {d[1]:02x}"
    cmds.setdefault(k, []).append(d)
print("OUT command groups (cmd sub): count, first nonzero-tail example")
for k,v in cmds.items():
    ex=v[0]; tail=ex.rstrip(b'\x00')
    print(f"  {k}: x{len(v)}  {tail.hex(' ')[:120]}")
# IN responses: group by cmd sub, count chunks, show non-ff payload
resp = collections.OrderedDict()
for e in ins:
    d=b(e); resp.setdefault(f"{d[0]:02x} {d[1]:02x}", []).append(d)
print("\nIN groups: chunks, total payload len (byte4), interesting chunks")
for k,v in resp.items():
    tot=sum(x[4] for x in v)
    interesting=[x for x in v if any(c not in (0,0xff) for c in x[5:5+x[4]])]
    print(f"  {k}: chunks={len(v)} payload={tot} nontrivial={len(interesting)}")
    for x in interesting[:2]:
        print("     ", x[:5+min(x[4],40)].hex(' '))
# is there any write-like OUT with payload?
print("\nOUT with nonzero bytes after byte 4:")
for e in outs:
    d=b(e)
    if any(d[5:]): print("  ", d.rstrip(b'\x00').hex(' ')[:150])
