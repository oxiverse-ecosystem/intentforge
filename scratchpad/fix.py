p='services/gateway/src/spell.rs'
lines=open(p).read().splitlines(keepends=True)
del lines[1027:1079]
open(p,'w').write(''.join(lines))
print('done', len(lines))
