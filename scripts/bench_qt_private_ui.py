#!/usr/bin/env python3
"""Whole-QBZ A/B probe on a private X server, using a read-only metadata copy.
Requires QBZ_PROBE_LIBRARY. Run only with no concurrent build or other probe.
"""
import argparse, json, os, subprocess, sys, time
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary',type=Path)
parser.add_argument('--baseline-binary',type=Path,help='Compare two builds with immediate artwork enabled in both')
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--repeats',type=int,default=2)
args=parser.parse_args()
results=[]
for repeat in range(args.repeats):
    for candidate in ((0,1) if repeat%2==0 else (1,0)):
        env=dict(os.environ,QBZ_GRID_IMMEDIATE_ART=str(1 if args.baseline_binary else candidate))
        binary = args.baseline_binary if args.baseline_binary and candidate == 0 else args.binary
        process=subprocess.Popen([sys.executable,str(Path(__file__).with_name('probe_qt_private_ui.py')),str(binary.resolve())],
                                 env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        root=None
        def command(value):
            process.stdin.write(value+'\n');process.stdin.flush()
            while True:
                line=process.stdout.readline()
                if not line:raise RuntimeError('probe stopped')
                if line.strip()=='ready':return
                print(line.strip(),flush=True)
        def wait_log(fragment):
            deadline=time.monotonic()+45
            while time.monotonic()<deadline:
                if process.poll() is not None:raise RuntimeError('probe exited')
                log=(root/'app.log').read_text(errors='replace')
                if fragment in log or (fragment=='QbzCore initialized' and 'Starting in offline-tolerant mode' in log):return
                time.sleep(.2)
            raise RuntimeError('missing startup marker: '+fragment)
        try:
            info=json.loads(process.stdout.readline());root=Path(info['root'])
            print(json.dumps({'repeat':repeat,'candidate':candidate,'profile':str(root)}),flush=True)
            wait_log('QbzCore initialized')
            time.sleep(2)
            command('click 588 520') # Start offline in the fresh profile.
            wait_log('phase=catch-up-active')
            time.sleep(2)
            command('click 464 110') # Local Library > Albums.
            wait_log('phase=albums-native')
            time.sleep(5)
            command('shot') # Baseline evidence only, outside measured interval.
            command('bench')
            log=(root/'app.log').read_text(errors='replace')
            for error in ('ReferenceError', 'TypeError', 'Cannot read', 'is not a type', 'Unable to assign'):
                if error in log:raise RuntimeError('invalid UI run: '+error+'; '+str(root/'app.log'))
            result=json.loads((root/'benchmark.json').read_text())
            result.update(repeat=repeat,profile=str(root),scenario=candidate,binary=str(binary.resolve()))
            results.append(result)
            args.output.write_text(json.dumps({'scope':'Whole QBZ, Xvfb/llvmpipe, copied metadata, no playback', 'runs':results},indent=2)+'\n')
        finally:
            if process.poll() is None:
                process.stdin.write('quit\n');process.stdin.flush()
                try:process.wait(timeout=10)
                except subprocess.TimeoutExpired:process.kill();process.wait()
