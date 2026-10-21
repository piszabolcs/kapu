# BLAKE3 tiszta Python referencia (a hivatalos reference_impl alapjan), csak a mintafajlhoz.
IV = [0x6A09E667,0xBB67AE85,0x3C6EF372,0xA54FF53A,0x510E527F,0x9B05688C,0x1F83D9AB,0x5BE0CD19]
PERM = [2,6,3,10,7,0,4,13,1,11,12,5,9,14,15,8]
CHUNK_START, CHUNK_END, PARENT, ROOT = 1, 2, 4, 8
M = 0xFFFFFFFF
def rotr(x,n): return ((x>>n)|(x<<(32-n)))&M
def g(s,a,b,c,d,x,y):
    s[a]=(s[a]+s[b]+x)&M; s[d]=rotr(s[d]^s[a],16)
    s[c]=(s[c]+s[d])&M;   s[b]=rotr(s[b]^s[c],12)
    s[a]=(s[a]+s[b]+y)&M; s[d]=rotr(s[d]^s[a],8)
    s[c]=(s[c]+s[d])&M;   s[b]=rotr(s[b]^s[c],7)
def rnd(s,m):
    g(s,0,4,8,12,m[0],m[1]); g(s,1,5,9,13,m[2],m[3]); g(s,2,6,10,14,m[4],m[5]); g(s,3,7,11,15,m[6],m[7])
    g(s,0,5,10,15,m[8],m[9]); g(s,1,6,11,12,m[10],m[11]); g(s,2,7,8,13,m[12],m[13]); g(s,3,4,9,14,m[14],m[15])
def compress(cv,block,counter,blen,flags):
    m=[int.from_bytes(block[i*4:i*4+4],'little') for i in range(16)]
    s=cv[:]+IV[:4]+[counter&M,(counter>>32)&M,blen,flags]
    for r in range(7):
        rnd(s,m)
        if r<6: m=[m[p] for p in PERM]
    for i in range(8): s[i]^=s[i+8]; s[i+8]^=cv[i]
    return s
def words(b): return [int.from_bytes(b[i*4:i*4+4],'little') for i in range(8)]
def chunk_out(data,counter):
    cv=IV[:]; blocks=[data[i:i+64] for i in range(0,max(len(data),1),64)] or [b'']
    for i,blk in enumerate(blocks):
        fl=(CHUNK_START if i==0 else 0)|(CHUNK_END if i==len(blocks)-1 else 0)
        if i<len(blocks)-1: cv=compress(cv,blk.ljust(64,b'\0'),counter,64,fl)[:8]
        else: return (cv,blk.ljust(64,b'\0'),counter,len(blk),fl)
def out_cv(o): return compress(o[0],o[1],o[2],o[3],o[4])[:8]
def parent_out(l,r): return (IV[:],b''.join(x.to_bytes(4,'little') for x in l+r),0,64,PARENT)
def blake3(data:bytes)->str:
    chunks=[data[i:i+1024] for i in range(0,len(data),1024)] or [b'']
    stack=[]; outs=None
    for idx,ch in enumerate(chunks):
        o=chunk_out(ch,idx)
        if idx==len(chunks)-1: outs=o; break
        cv=out_cv(o); total=idx+1
        while total&1==0: cv=out_cv(parent_out(stack.pop(),cv)); total>>=1
        stack.append(cv)
    o=outs
    while stack: o=parent_out(stack.pop(),out_cv(o))
    s=compress(o[0],o[1],o[2],o[3],o[4]|ROOT)
    return b''.join(x.to_bytes(4,'little') for x in s[:8]).hex()
if __name__=='__main__':
    assert blake3(b'')=='af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262'
    print('abc', blake3(b'abc'))
    pat=lambda n: bytes(i%251 for i in range(n))
    print('1024', blake3(pat(1024))); print('1025', blake3(pat(1025)))
    print('8192', blake3(pat(8192))[:16])
