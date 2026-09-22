#!/usr/bin/env python3
"""Private X11 QBZ probe. Copies library metadata; never controls the user's display.
Commands on stdin: shot, click X Y, wheel N, key NAME, stats, quit.
"""
import ctypes as C
from ctypes.util import find_library
import json, os, signal, sqlite3, subprocess, sys, tempfile, time
from pathlib import Path
import shutil
from PIL import ImageGrab

root=Path(tempfile.mkdtemp(prefix='qbz-private-ui-'))
for folder in ('config','data/qbz/users/0','cache','state','run'):
    (root/folder).mkdir(parents=True,exist_ok=True,mode=0o700)
origin=Path.home()/'.local/share/qbz'
library = Path(os.environ['QBZ_PROBE_LIBRARY'])
for source, target in ((library, root/'data/qbz/users/0/library.db'),
                       (origin/'legal_settings.db', root/'data/qbz/legal_settings.db')):
    src=sqlite3.connect(f'file:{source}?mode=ro',uri=True)
    dst=sqlite3.connect(target)
    src.backup(dst); dst.close(); src.close()
shutil.copytree(origin/'thumbnails',root/'data/qbz/thumbnails')
dpr=int(os.environ.get('QBZ_PROBE_DPR','1'))
if dpr not in (1,2): raise ValueError('QBZ_PROBE_DPR must be 1 or 2')
xvfb=subprocess.Popen(['Xvfb','-displayfd','1','-screen','0',f'{1280*dpr}x{900*dpr}x24','-nolisten','tcp'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
display=':'+xvfb.stdout.readline().decode().strip()
x=C.CDLL(find_library('X11')); xt=C.CDLL(find_library('Xtst'))
x.XOpenDisplay.argtypes=[C.c_char_p]; x.XOpenDisplay.restype=C.c_void_p
d=x.XOpenDisplay(display.encode())
x.XFlush.argtypes=[C.c_void_p]; x.XCloseDisplay.argtypes=[C.c_void_p]
x.XStringToKeysym.argtypes=[C.c_char_p]; x.XStringToKeysym.restype=C.c_ulong
x.XKeysymToKeycode.argtypes=[C.c_void_p,C.c_ulong]; x.XKeysymToKeycode.restype=C.c_uint
xt.XTestFakeMotionEvent.argtypes=[C.c_void_p,C.c_int,C.c_int,C.c_int,C.c_ulong]
xt.XTestFakeButtonEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
xt.XTestFakeKeyEvent.argtypes=[C.c_void_p,C.c_uint,C.c_int,C.c_ulong]
env=dict(os.environ,QT_SCALE_FACTOR=str(dpr),DISPLAY=display,QT_QPA_PLATFORM='xcb',QSG_RHI_BACKEND='opengl',LIBGL_ALWAYS_SOFTWARE='1',RUST_LOG='info')
for var,folder in [('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache'),('XDG_STATE_HOME','state'),('XDG_RUNTIME_DIR','run')]:env[var]=str(root/folder)
env.pop('WAYLAND_DISPLAY',None);env.pop('DBUS_SESSION_BUS_ADDRESS',None)
# This is an offline-library benchmark. Fail external HTTP promptly inside
# this child instead of letting bundle/CDN latency enter startup or CPU totals.
for proxy in ('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy'):
    env[proxy]='http://127.0.0.1:9'
env['NO_PROXY']=env['no_proxy']='127.0.0.1,localhost'

log=(root/'app.log').open('w')
app=subprocess.Popen(['dbus-run-session','--',str(Path(sys.argv[1]).resolve())],env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
print(json.dumps({'root':str(root),'display':display,'supervisor':app.pid}),flush=True)
def button(n):
    xt.XTestFakeButtonEvent(d,n,1,0);xt.XTestFakeButtonEvent(d,n,0,0);x.XFlush(d)
def key(name):
    keys=[x.XKeysymToKeycode(d,x.XStringToKeysym(k.encode())) for k in name.split('+')]
    for k in keys:xt.XTestFakeKeyEvent(d,k,1,0)
    for k in reversed(keys):xt.XTestFakeKeyEvent(d,k,0,0)
    x.XFlush(d)
def process_stats():
    for p in Path('/proc').iterdir():
        if not p.name.isdigit():continue
        try:
            e=(p/'environ').read_bytes()
            if b'XDG_RUNTIME_DIR='+str(root/'run').encode()+b'\0' not in e:continue
            if (p/'exe').resolve() != Path(sys.argv[1]).resolve():continue
            st=(p/'stat').read_text().split(') ',1)[1].split()
            rss=next(s for s in (p/'smaps_rollup').read_text().splitlines() if s.startswith('Rss:'))
            return {'pid':int(p.name),'cpu_s':(int(st[11])+int(st[12]))/os.sysconf('SC_CLK_TCK'),'rss_mib':int(rss.split()[1])/1024}
        except (FileNotFoundError,ProcessLookupError,PermissionError):pass
    raise RuntimeError('QBZ process is absent')

def benchmark():
    # Real wheel events, no capture or logging per frame. Keep the pointer
    # inside the grid. Each fresh profile follows the same finite route.
    xt.XTestFakeMotionEvent(d,-1,750*dpr,400*dpr,0);x.XFlush(d)
    start=process_stats(); peak=start['rss_mib']; samples=[]
    for direction in (5,4,5,4):
        for step in range(40):
            button(direction);time.sleep(.1)
            point=process_stats();peak=max(peak,point['rss_mib']);samples.append(point)
    scrolled=process_stats()
    for second in range(60):
        time.sleep(1);point=process_stats();peak=max(peak,point['rss_mib']);samples.append(point)
    end=process_stats()
    result={'dpr':dpr,'start':start,'scroll_end':scrolled,'end':end,'peak_mib':peak,
            'scroll_cpu_s':scrolled['cpu_s']-start['cpu_s'],
            'idle_cpu_s':end['cpu_s']-scrolled['cpu_s'],
            'candidate':os.environ.get('QBZ_GRID_IMMEDIATE_ART','1'),
            'samples':samples}
    (root/'benchmark.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps({k:v for k,v in result.items() if k!='samples'}),flush=True)

try:
    for line in sys.stdin:
        args=line.strip().split()
        if not args:continue
        if args[0]=='shot':
            path=root/'screen.png';ImageGrab.grab(xdisplay=display).save(path);print(path,flush=True)
        elif args[0]=='click':
            xt.XTestFakeMotionEvent(d,-1,int(args[1])*dpr,int(args[2])*dpr,0);button(1)
        elif args[0]=='wheel':
            n=int(args[1])
            for _ in range(abs(n)):button(5 if n>0 else 4);time.sleep(.016)
        elif args[0]=='key':key(args[1])
        elif args[0]=='stats':print(json.dumps(process_stats()),flush=True)
        elif args[0]=='bench':benchmark()
        elif args[0]=='quit':break
        print('ready',flush=True)
finally:
    try:os.killpg(app.pid,signal.SIGTERM)
    except ProcessLookupError:pass
    try:app.wait(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(app.pid,signal.SIGKILL);app.wait()
    x.XCloseDisplay(d);xvfb.terminate();xvfb.wait(timeout=5);log.close()
