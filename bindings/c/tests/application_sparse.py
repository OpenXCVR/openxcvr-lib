#!/usr/bin/env python3
"""Public optional C sparse ABI: bounds, ownership, and Rust fixture equivalence."""
import ctypes as c
import itertools
import subprocess
import sys

lib = c.CDLL(sys.argv[1])
class Range(c.Structure):
    _fields_ = [(n, c.c_uint8) for n in ('address', 'bank', 'page', 'offset')] + [('len', c.c_uint16)]
class Request(c.Structure):
    _fields_ = [('size', c.c_size_t), ('version', c.c_uint32), ('major', c.c_uint8), ('minor', c.c_uint8)]
class Footprint(c.Structure):
    _fields_ = Request._fields_ + [('count', c.c_size_t), ('ranges', c.POINTER(Range))]
class Supplied(c.Structure):
    _fields_ = [('range', Range), ('bytes', c.POINTER(c.c_uint8)), ('len', c.c_size_t)]
class SparseRequest(c.Structure):
    _fields_ = Request._fields_ + [('ranges', c.POINTER(Supplied)), ('count', c.c_size_t)]
class Record(c.Structure):
    _fields_ = [(n,c.c_uint8) for n in ('slot','has','host','media','hl','ml','assign','unsupported')] + [(n,c.c_uint32) for n in ('state','hs','ms','hls','mls')] + [('hn',c.c_char_p),('mn',c.c_char_p)]
class Result(c.Structure):
    _fields_ = Request._fields_ + [('count', c.c_size_t), ('records', c.POINTER(Record))]
fp_query=lib.openxcvr_application_records_footprint
fp_query.argtypes=[c.POINTER(Request),c.POINTER(c.POINTER(Footprint))];fp_query.restype=c.c_int
fp_free=lib.openxcvr_application_records_footprint_free;fp_free.argtypes=[c.POINTER(Footprint)]
decode=lib.openxcvr_application_records_from_ranges
decode.argtypes=[c.POINTER(SparseRequest),c.POINTER(c.POINTER(Result))];decode.restype=c.c_int
free=lib.openxcvr_application_records_free;free.argtypes=[c.POINTER(Result)]

def request(ty, rev):
    r=ty();r.size=c.sizeof(ty);r.version=1;r.major=rev>>4;r.minor=rev&15
    return r

def dump(out):
    assert out.contents.count==15 and out.contents.version==1 and out.contents.size==c.sizeof(Result)
    lines=[]
    for p in out.contents.records[:15]:
        values=[p.slot,p.state,p.has,p.host,p.media,p.hl,p.ml,p.assign,p.hs,p.ms,p.hls,p.mls,p.unsupported,(p.hn or b'-').decode(),(p.mn or b'-').decode()]
        lines.append('|'.join(map(str,values)))
    return '\n'.join(lines)+'\n'

for rev in (0x40,0x50,0x51,0x52,0x53):
    r=request(Request,rev);out=c.POINTER(Footprint)()
    assert fp_query(c.byref(r),c.byref(out))==0
    assert out.contents.size==c.sizeof(Footprint) and out.contents.version==1
    assert [(v.address,v.bank,v.page,v.offset,v.len) for v in out.contents.ranges[:out.contents.count]]==[(80,0,0,1,1),(80,0,0,85,33),(80,0,1,223,28)]
    r.major=0  # result-owned, not a view of the request
    assert out.contents.major==rev>>4
    fp_free(out)
r=request(Request,0x30);out=c.POINTER(Footprint)()
assert fp_query(c.byref(r),c.byref(out))==5 and not out  # unsupported revision
r=request(Request,0x40)
for field,value in [('size',0),('version',2)]:
    bad=request(Request,0x40);setattr(bad,field,value)
    assert fp_query(c.byref(bad),c.byref(out))==1 and not out
assert fp_query(None,c.byref(out))==1 and not out
assert fp_query(c.byref(r),None)==1
fp_free(None);free(None)

count=0
for rev,media,scenario in itertools.product((0x40,0x50,0x51,0x52,0x53,0x54,0x30),(0,1,2,3,4,5,0x40),range(12)):
    lo=bytearray(128);hi=bytearray(128);lo[1]=rev;lo[85]=media
    for i in range(8):lo[86+4*i:90+4*i]=bytes([0x1a,1,0x44,0x11])
    for i in range(7):hi[95+4*i:99+4*i]=bytes([0x1a,1,0x22,0x55])
    if scenario==1:lo[114]=255
    elif scenario==2:hi[95]=255
    elif scenario==5:lo[86]=254;lo[87]=254
    elif scenario==6:lo[86]=0;lo[87]=0
    elif scenario==7:lo[88]=9;lo[89]=255
    elif scenario==8:lo[1]=0
    elif scenario==11:hi[119]=255
    # Split to one-byte fragments; no padding or full page is supplied to C.
    data=[];spans=[]
    for page,offset,end in ((0,1,2),(0,85,118),(1,223,251)):
        for b in range(offset,end):
            if (page==0 and (scenario==9 or (scenario==10 and b>=117))) or (page==1 and (scenario==3 or (scenario==4 and b>=250))):continue
            buf=(c.c_uint8*1)((lo if page==0 else hi)[b if page==0 else b-128]);data.append(buf)
            spans.append(Supplied(Range(80,0,page,b,1),buf,1))
    array=(Supplied*len(spans))(*spans)
    r=request(SparseRequest,rev);r.ranges=array;r.count=len(spans)
    out=c.POINTER(Result)()
    assert decode(c.byref(r),c.byref(out))==0
    for buf in data:buf[0]^=255  # owned records and names survive input mutation
    actual=dump(out)
    expected=subprocess.check_output([sys.argv[2],str(rev),str(media),str(scenario),'sparse'],text=True)
    assert actual==expected,(rev,media,scenario,actual,expected)
    free(out);count+=1
# Invalid counted pointers, bounds, duplicate/overlapping and bad selection.
r=request(SparseRequest,0x40);out=c.POINTER(Result)()
assert decode(None,c.byref(out))==1 and not out
assert decode(c.byref(r),None)==1
for field,value in [('size',0),('version',2),('count',257)]:
    bad=request(SparseRequest,0x40);setattr(bad,field,value)
    assert decode(c.byref(bad),c.byref(out))==1 and not out
r.count=1
assert decode(c.byref(r),c.byref(out))==1 and not out
buf=(c.c_uint8*2)(0x40,0)
for spans in ([Supplied(Range(80,0,0,1,1),None,1)], [Supplied(Range(80,0,0,1,2),buf,1)], [Supplied(Range(80,0,0,127,2),buf,2)], [Supplied(Range(80,0,1,1,1),buf,1)], [Supplied(Range(80,0,0,1,1),buf,1)]*2):
    array=(Supplied*len(spans))(*spans);r.ranges=array;r.count=len(spans)
    assert decode(c.byref(r),c.byref(out))==1 and not out
print(f'Sparse Rust/C equivalence: {count} fixtures, {count*15} records; footprint, bounds and ownership passed')
