#!/usr/bin/env python3
"""Actual A2NE Chao Bounce task scheduler parity with an explicit numerical BIOS.

The original Thumb executes all gameplay/task creation. Only presentation BIOS
decompression/copies/affine outputs and audio are modeled. Sqrt/ArcTan2 use a
scalar reference independently checked in bulk against pinned mGBA C.
"""
from __future__ import annotations
import argparse,hashlib,json,math,shutil,struct,subprocess,sys,time
from pathlib import Path
sys.dont_write_bytecode=True

SHA1="2c7da8e1c9af85986eecf4effae39197cdac9abf"
LENGTH=191340
BIOS_SHA="5482575dfb1849c2672144e7c5b13737cb1f920608dc76d03f509f9d5d48fb97"
RETURN=0x0203f000
TASKS=0x03004400
CTX=0x03001ad0

def signed(value,bits=32):
 value&=(1<<bits)-1
 return value-(1<<bits)if value&(1<<(bits-1))else value
def div(a,b):return (-1 if (a<0)^(b<0)else 1)*(abs(a)//abs(b))if b else 0
def atan(ratio):
 squared=-((ratio*ratio)>>14);p=0xa9
 for k in[0x390,0x91c,0xfb6,0x16aa,0x2081,0x3651,0xa2f9]:p=((p*squared)>>14)+k
 return(ratio*p)>>16
def atan2(x,y):
 if y==0:return 0x8000 if x<0 else 0
 if x==0:return 0xc000 if y<0 else 0x4000
 horizontal=abs(x)>abs(y)or(abs(x)==abs(y)and not(x<0 and y<0))
 value=((0x8000 if x<0 else 0x10000 if y<0 else 0)+atan(div(y<<14,x)))if horizontal else((0xc000 if y<0 else 0x4000)-atan(div(x<<14,y)))
 return value&0xffff

class Oracle:
 def __init__(self,payload):
  from unicorn import Uc,UC_ARCH_ARM,UC_MODE_THUMB,UC_HOOK_CODE,UC_HOOK_INTR
  from unicorn import arm_const as arm
  self.arm=arm;self.uc=Uc(UC_ARCH_ARM,UC_MODE_THUMB)
  for base,size in[(0x02000000,0x40000),(0x03000000,0x10000),(0x04000000,0x1000),(0x05000000,0x1000),(0x06000000,0x20000),(0x07000000,0x1000)]:self.uc.mem_map(base,size)
  self.uc.mem_write(0x02000000,payload);self.calls={};self.sounds=0;self.graphics=0;self.vectors={};self.stopped=False;self.poses=set()
  for address in[RETURN,0x020104e0,0x02010678,0x02010614,0x0201064c]:self.uc.hook_add(UC_HOOK_CODE,self.code,begin=address,end=address)
  self.uc.hook_add(UC_HOOK_INTR,self.bios)
 def code(self,u,address,size,data):
  if address==RETURN:self.stopped=True;u.emu_stop()
  elif address in[0x020104e0,0x02010678,0x02010614,0x0201064c]:
   self.sounds+=1;u.reg_write(self.arm.UC_ARM_REG_PC,u.reg_read(self.arm.UC_ARM_REG_LR))
 def call(self,address,r0=0):
  a=self.arm;u=self.uc
  for i in range(13):u.reg_write(getattr(a,f"UC_ARM_REG_R{i}"),0)
  u.reg_write(a.UC_ARM_REG_R0,r0);u.reg_write(a.UC_ARM_REG_SP,0x0300f000);u.reg_write(a.UC_ARM_REG_LR,RETURN|1);self.stopped=False
  try:u.emu_start(address|1,RETURN+2,count=1000000)
  except Exception as exc:raise RuntimeError(f"native entry{address:#x} pc{u.reg_read(a.UC_ARM_REG_PC):#x} lr{u.reg_read(a.UC_ARM_REG_LR):#x}")from exc
  if not self.stopped:raise RuntimeError(f"native callback{address:#x} did not return")
 def bios(self,u,interrupt,data):
  a=self.arm;pc=u.reg_read(a.UC_ARM_REG_PC);op=bytes(u.mem_read(pc-2,2))
  if interrupt!=2 or op[1]!=0xdf:raise RuntimeError(f"unexpected native interrupt{interrupt}:{op.hex()}")
  service=op[0];self.calls[service]=self.calls.get(service,0)+1
  x=u.reg_read(a.UC_ARM_REG_R0);y=u.reg_read(a.UC_ARM_REG_R1);n=u.reg_read(a.UC_ARM_REG_R2)
  if service==6:
   quotient=div(signed(x),signed(y));u.reg_write(a.UC_ARM_REG_R0,quotient&0xffffffff);u.reg_write(a.UC_ARM_REG_R1,(signed(x)-quotient*signed(y))&0xffffffff);u.reg_write(a.UC_ARM_REG_R3,abs(quotient)&0xffffffff)
  elif service==8:
   value=math.isqrt(x);self.vectors[('S',x,0)]=value;u.reg_write(a.UC_ARM_REG_R0,value)
  elif service==10:
   value=atan2(signed(x),signed(y));self.vectors[('A',signed(x),signed(y))]=value;u.reg_write(a.UC_ARM_REG_R0,value)
  elif service in[0xb,0xc]:
   size=4 if service==0xc or n&0x04000000 else 2;count=(n&0x1fffff)*size
   if count>0x10000:raise RuntimeError("unbounded graphic copy")
   value=bytes(u.mem_read(x,size if n&0x01000000 else count));u.mem_write(y,value*(count//size)if n&0x01000000 else value);self.graphics+=1
  elif service in[0x11,0x12]:
   header=bytes(u.mem_read(x,4))
   if header[0]!=0x10:raise RuntimeError("unknown LZ stream")
   length=int.from_bytes(header[1:4],'little');out=bytearray();at=x+4
   if not 0<length<0x20000:raise RuntimeError("unbounded graphic stream")
   while len(out)<length:
    flag=u.mem_read(at,1)[0];at+=1
    for bit in range(7,-1,-1):
     if len(out)==length:break
     if flag&(1<<bit):
      first,second=u.mem_read(at,2);at+=2;distance=((first&15)<<8|second)+1
      if distance>len(out):raise RuntimeError("invalid graphic back reference")
      for _ in range((first>>4)+3):
       if len(out)==length:break
       out.append(out[-distance])
     else:out.extend(u.mem_read(at,1));at+=1
   u.mem_write(y,bytes(out));self.graphics+=1
  elif service==0xf:
   # OBJ affine output is a renderer-only buffer; gameplay never reads it.
   stride=u.reg_read(a.UC_ARM_REG_R3)
   if n!=1 or stride!=8:raise RuntimeError("unexpected affine shape")
   for i,value in enumerate([256,0,0,256]):u.mem_write(y+i*stride,struct.pack('<h',value))
   self.graphics+=1
  else:raise RuntimeError(f"unsupported native BIOS service{service:#x}")
 def begin(self,stage):
  u=self.uc;u.mem_write(0x03000000,bytes(0x10000));u.mem_write(0x07000000,bytes(0x1000));u.mem_write(0x03001ad2,b'\x03')
  u.mem_write(0x03001af0,bytes([1])*26);u.mem_write(0x03001b0a,b'\x1a');u.mem_write(0x03001ad1,bytes([stage]))
  if stage>2:u.mem_write(0x03001af6,b'\x02')
  # Native D538 builds the actual task list, including new child callbacks.
  self.call(0x0200d538)
  self.target_ids={}
 def tasks(self):
  result=[];index=0
  for _ in range(72):
   raw=bytes(self.uc.mem_read(TASKS+index*36,36));result.append((index,raw));index=raw[6]
   if index==0:return result
  raise RuntimeError("native linked task list escaped bound")
 def actor(self,callback,rows):
  selected=[raw for _,raw in rows if struct.unpack_from('<I',raw)[0]in callback]
  if len(selected)!=1:raise AssertionError(f"native callback selection{callback} count{len(selected)}")
  return selected[0]
 def round(self,stage,mode,trace,size):
  self.begin(stage);u=self.uc;cleared=set()
  for frame in range(len(trace)//size):
   if mode==7:
    current_stage=u.mem_read(0x03001ad1,1)[0];chao=struct.unpack('<I',bytes(u.mem_read(0x03001adc,4)))[0]
    if current_stage not in cleared and struct.unpack('<I',bytes(u.mem_read(chao,4)))[0]==0x0200d959:
     cleared.add(current_stage)
     for index,raw_actor in self.tasks():
      if struct.unpack_from('<I',raw_actor)[0]in[0x0200dd61,0x0200dcc1]:self.call(0x02000ed4,TASKS+index*36)
     u.mem_write(0x03001af0,bytes(26));u.mem_write(0x03001b0a,b'\0');self.target_ids={}
   actual=trace[frame*size:(frame+1)*size];pressed,held=struct.unpack_from('<HH',actual)
   u.mem_write(0x030043f0,struct.pack('<H',held));u.mem_write(0x030043f4,struct.pack('<H',pressed))
   main=struct.unpack('<I',bytes(u.mem_read(0x03003330,4)))[0]
   if main not in[0x0200f5ad,0x0200f621,0x0200f58d,0x0200d499]:raise RuntimeError(f"unexpected native scene callback{main:#x}")
   self.call(main&~1)
   raw=bytes(u.mem_read(CTX,0x50));ctx=raw[1:3]+raw[4:6]+raw[0x18:0x1a]+bytes(u.mem_read(0x0300333f,1))+raw[0x3a:0x3c]+raw[0x48:0x50]+raw[0x10:0x18]
   if ctx!=actual[4:29]:
    diffs=[i for i,(left,right)in enumerate(zip(ctx,actual[4:29]))if left!=right]
    raise AssertionError(f"stage{stage} mode{mode} frame{frame} context offsets{diffs}: native{ctx.hex()} rust{actual[4:29].hex()}")
   scene=struct.unpack('<I',bytes(u.mem_read(0x03003330,4)))[0]
   if scene==0x02001bf9:
    if actual[-1]!=1:raise AssertionError('native return was not acknowledged by Rust core')
    continue # The original restores the unrelated Garden actor list here.
   rows=self.tasks();chao_ptr=struct.unpack('<I',raw[12:16])[0];spring_ptr=struct.unpack('<I',raw[8:12])[0]
   for _,native_actor in rows:
    descriptor=struct.unpack_from('<I',native_actor,16)[0]
    if 0x0202e4c8<=descriptor<=0x0202e560 and(descriptor-0x0202e4c8)%8==0:self.poses.add((descriptor-0x02000000,struct.unpack_from('<H',native_actor,20)[0],native_actor[22],native_actor[23],native_actor[24]))
   if actual[29:33]==struct.pack('<I',0x0200dc6c):
    self.target_ids={}
    continue # D538 has just allocated new tasks; callbacks initialize next frame.
   actors=[bytes(u.mem_read(chao_ptr,36)),bytes(u.mem_read(spring_ptr,36)),self.actor([0x0200deed,0x0200de99],rows)]
   at=29
   for name,native,mask in zip(['chao','spring','master'],actors,[list(range(4))+list(range(7,16))+[20,21,25,26,29,30,31,32,33,34,35],list(range(4))+list(range(7,16))+[20,21,25,26,28,29,30,31,32,33,34,35],list(range(4))+list(range(8,16))+[25,26,34,35]]):
    rust=actual[at:at+36];at+=36
    # Native Thumb function pointers carry bit0; core stores code addresses.
    native=bytes([native[0]&0xfe])+native[1:]
    diffs=[i for i in mask if native[i]!=rust[i]]
    if diffs:raise AssertionError(f"stage{stage} mode{mode} frame{frame} {name} offsets{diffs} native{native.hex()} rust{rust.hex()}")
   active=dict(rows)
   for index,native in rows:
    callback=struct.unpack_from('<I',native)[0]
    if callback==0x0200dd61 and index not in self.target_ids:self.target_ids[index]=native[25]*13+native[26]
   for index in range(26):
    rust=actual[at+1:at+37];present=actual[at];at+=37
    native=next((active[i]for i,cell in self.target_ids.items()if cell==index and i in active),None)
    if bool(present)!=(native is not None):raise AssertionError(f"stage{stage} mode{mode} frame{frame} target{index} presence")
    if native:
     native=bytes([native[0]&0xfe])+native[1:]
     mask=list(range(4))+list(range(8,16))+[25,26,29,31,32,33,34,35]
     if (struct.unpack_from('<H',native,14)[0]+24)&0xffff<=224:mask+=[20,21]
     diffs=[i for i in mask if native[i]!=rust[i]]
     if diffs:raise AssertionError(f"stage{stage} mode{mode} frame{frame} target{index} offsets{diffs} native{native.hex()} rust{rust.hex()}")
   clouds={native[25]:native for _,native in rows if struct.unpack_from('<I',native)[0]==0x0200efa1}
   for index in range(3):
    rust=actual[at:at+36];at+=36;native=clouds[index];mask=list(range(8,16))+[25,34,35]
    if any(native[i]!=rust[i]for i in mask):raise AssertionError(f"stage{stage} mode{mode} frame{frame} cloud{index}")
   bumpers={native[28]:native for _,native in rows if struct.unpack_from('<I',native)[0]==0x0200f11d}
   for index in range(2):
    present=actual[at];rust=actual[at+1:at+37];at+=37;native=bumpers.get(index)
    if bool(present)!=(native is not None):raise AssertionError(f"stage{stage} mode{mode} frame{frame} bumper{index} presence")
    if native:
     mask=[7]+list(range(8,16))+[25,26]+list(range(28,36));diffs=[i for i in mask if native[i]!=rust[i]]
     if diffs:raise AssertionError(f"stage{stage} mode{mode} frame{frame} bumper{index} offsets{diffs}")
   if actual[at:at+26]!=raw[0x20:0x3a]:raise AssertionError(f"stage{stage} mode{mode} frame{frame} target kind differs")
   at+=26
   scene=struct.unpack('<I',bytes(u.mem_read(0x03003330,4)))[0]
   if bool(actual[at])!=(scene==0x02001bf9):raise AssertionError(f"stage{stage} mode{mode} frame{frame} completion differs")
   if at+1!=size:raise RuntimeError("Rust projection shape changed")

def main():
 p=argparse.ArgumentParser(description=__doc__)
 for name in['payload','output-dir','bios-reference']:p.add_argument('--'+name,required=True,type=Path)
 p.add_argument('--unicorn-dir',type=Path);p.add_argument('--frames',type=int,default=2000);a=p.parse_args()
 payload=a.payload.read_bytes()
 if len(payload)!=LENGTH or hashlib.sha1(payload).hexdigest()!=SHA1:raise ValueError('A2NE payload identity mismatch')
 if hashlib.sha256(a.bios_reference.read_bytes()).hexdigest()!=BIOS_SHA:raise ValueError('independent BIOS reference mismatch')
 if not 1<=a.frames<=20000:p.error('frames must be1..20000')
 output=a.output_dir.resolve()
 if output.exists()or'.local'not in[x.lower()for x in output.parts]:raise ValueError('new private.local output required')
 if a.unicorn_dir:sys.path.insert(0,str(a.unicorn_dir))
 import unicorn
 if unicorn.__version__!='2.1.4':raise ValueError('requires Unicorn2.1.4')
 cargo=shutil.which('cargo')
 if not cargo:raise ValueError('activate Rust')
 output.mkdir(parents=True);core=Path(__file__).resolve().parents[1]
 manifest='[package]\nname="native-bounce-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ntiny-garden-core={path='+json.dumps(str(core).replace('\\','/'))+'}\n[[bin]]\nname="native-bounce-proof"\npath='+json.dumps(str(core/'tools/native_bounce_probe.rs').replace('\\','/'))+'\n'
 (output/'Cargo.toml').write_text(manifest,encoding='utf-8');subprocess.run([cargo,'build','--offline','--quiet','--manifest-path',str(output/'Cargo.toml'),'--target-dir',str(output/'target')],check=True,timeout=120)
 exe=output/'target/debug'/('native-bounce-proof.exe'if sys.platform=='win32'else'native-bounce-proof')
 cases=[(stage,mode,a.frames)for stage in range(4)for mode in range(8)]
 encoded=b''.join(struct.pack('<BBH',*case)for case in cases);trace=subprocess.run([str(exe),str(a.payload.resolve())],input=encoded,capture_output=True,check=True,timeout=60).stdout
 oracle=Oracle(payload);offset=0;total=0;started=time.monotonic()
 for stage,mode,_ in cases:
  frames,size=struct.unpack_from('<IH',trace,offset);offset+=6;part=trace[offset:offset+frames*size];offset+=frames*size
  if not frames or len(part)!=frames*size:raise RuntimeError('incomplete Rust trace')
  oracle.round(stage,mode,part,size);total+=frames
 if offset!=len(trace):raise RuntimeError('surplus Rust trace')
 vectors=list(oracle.vectors);request=''.join(f'{kind} {x} {y}\n'for kind,x,y in vectors);reference=subprocess.run([str(a.bios_reference.resolve())],input=request,text=True,capture_output=True,check=True,timeout=60).stdout.splitlines()
 if list(map(int,reference))!=[oracle.vectors[v]for v in vectors]:raise AssertionError('independent BIOS numerical mismatch')
 result={'result':'pass','full_task_traces':len(cases),'native_frames':total,'bios_calls':oracle.calls,'independent_bios_vectors':len(vectors),'payload_sha1':SHA1,'audio_callbacks_skipped':oracle.sounds,'render_bios_modeled':oracle.graphics,'source_sha256':hashlib.sha256((core/'src/native_bounce.rs').read_bytes()).hexdigest(),'probe_sha256':hashlib.sha256((core/'tools/native_bounce_probe.rs').read_bytes()).hexdigest(),'verifier_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'corpus_sha256':hashlib.sha256(encoded).hexdigest(),'elapsed_seconds':round(time.monotonic()-started,3),'limits':['Native graphics/audio output and authentic BIOS machine code not proven','No MMO adapter or durable reward transaction proven']}
 (output/'native_sprite_poses.json').write_text(json.dumps([{'descriptor_offset':f'0x{offset:x}','tile':tile,'flags16':f16,'flags17':f17,'packed_palette_layer':count}for offset,tile,f16,f17,count in sorted(oracle.poses)],indent=2)+'\n',encoding='utf-8')
 (output/'result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result,indent=2))
if __name__=='__main__':main()
