#!/usr/bin/env python3
"""Isolated A/B microbenchmark of the real RoundedImage/Skeleton components.

Measures the reveal/decode path, NOT the complete application's memory budget
or the Rust thumbnail pipeline. Uses existing thumbs read-only; fixtures and
logs are temporary. Run without a Cargo build or screen recording in progress.
"""
import argparse
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import time
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
QML = '/usr/lib64/qt6/bin/qml'

def run(root, candidate, dpr, display, repeat):
    env = dict(os.environ, DISPLAY=display, QT_QPA_PLATFORM='xcb',
               QSG_RHI_BACKEND='opengl', LIBGL_ALWAYS_SOFTWARE='1', QT_SCALE_FACTOR=str(dpr))
    env.pop('WAYLAND_DISPLAY', None)
    log = root / f'run-{dpr}-{repeat}-{candidate}.log'
    bench = root / 'Bench.qml'
    bench.write_text(re.sub(r'property bool candidate: (?:true|false)',
                             'property bool candidate: ' + str(candidate).lower(), bench.read_text()))
    with log.open('w') as stream:
        p = subprocess.Popen([QML, '-I', str(root / 'imports'), '-f', str(root / 'Bench.qml')],
                             env=env, stdout=stream, stderr=subprocess.STDOUT)
        rss_peak = cpu = rss_last = 0
        try:
            deadline = time.monotonic() + 25
            while p.poll() is None and time.monotonic() < deadline:
                try:
                    stat = Path(f'/proc/{p.pid}/stat').read_text().split(') ', 1)[1].split()
                    cpu = (int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK')
                    rss = Path(f'/proc/{p.pid}/smaps_rollup').read_text()
                    rss_last = int(next(line.split()[1] for line in rss.splitlines() if line.startswith('Rss:')))
                    rss_peak = max(rss_peak, rss_last)
                except (FileNotFoundError, ProcessLookupError):
                    pass
                time.sleep(.05)
            if p.poll() is None:
                raise RuntimeError('benchmark did not finish')
        finally:
            if p.poll() is None:
                p.kill(); p.wait()
    content = log.read_text()
    if p.returncode or any(s in content for s in ('ReferenceError', 'TypeError', 'is not a type', 'Cannot open', 'failed to create')):
        raise RuntimeError(content)
    match = re.search(r'BENCH_RESULT (\d+) (\d+) (\d+) (\d+) (\d+)', content)
    if not match or int(match[1]) < 100 or int(match[2]) < 100 or int(match[3]) < 100 or int(match[4]) < 100 or int(match[5]) != 0:
        raise RuntimeError('benchmark did not render enough images/frames: '+content)
    return dict(ready_events=int(match[1]), frames=int(match[2]), revealed=int(match[3]), handed_over=int(match[4]), candidate=candidate, dpr=dpr, cpu_s=cpu, peak_mib=rss_peak/1024,
                end_mib=rss_last/1024, log=str(log))

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--thumbs', type=Path, default=Path.home()/'.local/share/qbz/thumbnails')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--repeats', type=int, default=3)
    args = parser.parse_args()
    root = Path(tempfile.mkdtemp(prefix='qbz-artwork-bench-'))
    module = root/'imports/com/blitzfc/qbz'; module.mkdir(parents=True)
    derivatives = {}; sources = []
    for src in sorted(args.thumbs.iterdir()):
        if len(sources) >= 96: break
        try:
            im = Image.open(src).convert('RGBA')
        except (OSError, ValueError):
            continue
        url = src.resolve().as_uri(); sources.append(url)
        for edge in (200, 400):
            scale = max(edge/im.width, edge/im.height)
            w, h = int(im.width*scale+.5), int(im.height*scale+.5)
            out = root/f'{len(sources)}-{w}x{h}.png'
            im.resize((w,h), Image.Resampling.LANCZOS).save(out)
            derivatives[f'{url}|{w}x{h}'] = out.as_uri()
    if len(sources) < 30: raise RuntimeError('need at least 30 existing thumbnails')
    (module/'qmldir').write_text('module com.blitzfc.qbz\nsingleton QbzShell 1.0 QbzShell.qml\nsingleton QbzPlayer 1.0 QbzPlayer.qml\nsingleton QbzLocal 1.0 QbzLocal.qml\nsingleton QbzSession 1.0 QbzSession.qml\n')
    (module/'QbzShell.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject { property string themeJson: ""; property int ambientMode: 0; property bool reduceMotion: false; property bool forceCanvasArt: false }')
    (module/'QbzLocal.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject { function artworkImmediateEnabled() { return true } }')
    (module/'QbzPlayer.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject { property bool npHasTrack: false }')
    (module/'QbzSession.qml').write_text('pragma Singleton\nimport QtQuick\nQtObject { property var paths: '+json.dumps(derivatives)+'; signal artScaledReady(string path, string scaled, int w, int h); function artScaledCached(p,w,h) { return paths[p+"|"+w+"x"+h] || "" } function artScaled(p,w,h) {} }')
    (root/'Bench.qml').write_text('''import QtQuick
import QtQuick.Window
import "'''+(ROOT/'crates/qbz-qt/qml/theme').as_uri()+'''" as Theme
import "'''+(ROOT/'crates/qbz-qt/qml/controls').as_uri()+'''" as Controls
Window {
    id: benchWindow; width: 1280; height: 900; visible: true; color: "#121212"
    property bool candidate: false
    property int readyEvents: 0
    property int revealEvents: 0
    property int handoverEvents: 0
    property int modeErrors: 0
    property int frames: 0
    onFrameSwapped: frames++
    Component.onCompleted: grid.model = 200
    property var sources: '''+json.dumps(sources)+'''
    ListView {
        id: grid; anchors.fill: parent; model: 0; cacheBuffer: 0; reuseItems: true
        delegate: Row {
            id: row; required property int index; height: 246; spacing: 10
            Repeater {
                model: 6
                delegate: Item {
                    required property int index; width: 200; height: 246
                    property bool candidate: false
                    property bool immediateReveal: false
                    Component.onCompleted: {
                        candidate = benchWindow.candidate && benchWindow.screen.devicePixelRatio <= 1
                        immediateReveal = candidate && benchWindow.screen.devicePixelRatio <= 1
                        artSource = Qt.binding(function () {
                            return benchWindow ? benchWindow.sources[(row.index*6+index)%benchWindow.sources.length] : ""
                        })
                    }
                    property string artSource: benchWindow ? benchWindow.sources[(row.index*6+index)%benchWindow.sources.length] : ""
                    Theme.RoundedImage { id: art; width: 200; height: 200; radius: 8; source: parent.artSource; gridArtwork: parent.candidate; onReadyChanged: if (ready && benchWindow) benchWindow.readyEvents++
                        onRevealedChanged: if (revealed && benchWindow) { benchWindow.revealEvents++; if (fadeMs !== (benchWindow.candidate && benchWindow.screen.devicePixelRatio <= 1 ? 0 : 200)) benchWindow.modeErrors++ }
                    }
                    Controls.QbzSkeleton { width: 200; height: 200; variant: "art";
                        coverSource: parent.candidate ? "" : parent.artSource;
                        coverReady: parent.candidate && art.ready;
                        handoverFadeMs: parent.immediateReveal ? 0 : 180; animated: false
                        onHandedOverChanged: if (handedOver && benchWindow) { benchWindow.handoverEvents++; if (handoverFadeMs !== (benchWindow.candidate && benchWindow.screen.devicePixelRatio <= 1 ? 0 : 180)) benchWindow.modeErrors++ }
                    }
                }
            }
        }
        SequentialAnimation on contentY {
            running: true
            PauseAnimation { duration: 500 }
            NumberAnimation { to: 18000; duration: 6000 }
            NumberAnimation { to: 0; duration: 6000 }
            PauseAnimation { duration: 1500 }
            ScriptAction { script: { console.log("BENCH_RESULT", benchWindow.readyEvents, benchWindow.frames, benchWindow.revealEvents, benchWindow.handoverEvents, benchWindow.modeErrors); grid.model = 0; Qt.callLater(function() { Qt.exit(0) }) } }
        }
    }
}''')
    xvfb = subprocess.Popen(['Xvfb','-displayfd','1','-screen','0','2560x1800x24','-nolisten','tcp'], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        display = ':'+xvfb.stdout.readline().decode().strip()
        rows=[]
        for dpr in (1,2):
            for repeat in range(args.repeats):
                for candidate in ([False,True] if repeat%2==0 else [True,False]):
                    result=run(root,candidate,dpr,display,repeat); rows.append(result)
                    print(json.dumps(result), flush=True)
        args.output.write_text(json.dumps({'scope':'RoundedImage + QbzSkeleton microbenchmark, Xvfb/llvmpipe; not whole QBZ', 'fixtures':str(root), 'runs':rows},indent=2)+'\n')
    finally:
        xvfb.terminate(); xvfb.wait(timeout=5)

if __name__ == '__main__': main()
