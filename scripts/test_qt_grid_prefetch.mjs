// Exercise the production range/report callbacks with fake native view geometry.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const base = new URL('../crates/qbz-qt/qml/views/', import.meta.url);
function fn(file, name) {
 const source=fs.readFileSync(new URL(file,base),'utf8');
 const begin=source.indexOf('function '+name+'('); assert.ok(begin>=0);
 const open=source.indexOf('{',begin); let depth=1;
 for(let i=open+1;i<source.length;i++) {
  if(source[i]==='{')depth++;
  if(source[i]==='}'&&!--depth)return '('+source.slice(begin,i+1)+')';
 }
 throw Error(name);
}
function call(file,name,ctx,...args) {return vm.runInNewContext(fn(file,name),ctx)(...args);}
let windows=[];
const rows=Array.from({length:200},(_,i)=>({kind:'album',artKey:'a'+i,imageUrl:'url'+i}));
const root={visibleRows:rows};
let grid={visible:true,width:880,cellWidth:220,cellHeight:266,height:532,contentY:2660};
call('LibraryView.qml','gridWindowReport',{root,grid,queueWindowReport:(...a)=>windows.push(a)});
assert.deepEqual(windows[0].filter((_,i)=>i!==2),[32,55,40]); // visible rows 10/11, ±2
let starts=0;
const timer={running:false,start(){starts++;this.running=true;}};
for(let i=0;i<50;i++)call('LibraryView.qml','queueWindowReport',{windowDebounce:timer},i,i+20);
assert.equal(starts,1,'continuous scroll must not postpone the flush');
assert.equal(timer.pendingFirst,49,'only the latest pending window survives');
Object.assign(root,{artMap:{},_artInbox:{},_artSent:{}});
const requests=[];
const ctx={root,artMap:root.artMap,QbzLibrary:{libraryArtworkWindow:s=>requests.push(JSON.parse(s))}};
call('LibraryView.qml','reportWindow',ctx,rows,32,55,40);
assert.equal(requests[0][0],'a40','visible art precedes buffered art');
assert.equal(requests[0].length,24);
call('LibraryView.qml','reportWindow',ctx,rows,32,55,40);
assert.equal(requests.length,1,'pending downloads are not duplicated every tick');
root._artSent.a40 -= 30000;
call('LibraryView.qml','reportWindow',ctx,rows,32,55,40);
assert.deepEqual(requests[1],['a40'],'a failed request can be retried on a later report');
root.artMap.a40='cached.png'; root._artSent.a40 -= 30000;
call('LibraryView.qml','reportWindow',ctx,rows,32,55,40);
assert.equal(requests.length,2,'resolved covers do not re-enter the bridge');
call('LibraryView.qml','reportWindow',ctx,[],0,-1);
assert.equal(Object.keys(root._artSent).length,0,'empty model releases request state');
// Native Local Library uses entry rows, not album indices. Two buffered rows
// of four albums must be requested even though they are outside the viewport.
let captured;
const entries=Array.from({length:40},(_,i)=>({t:1,items:Array.from({length:4},(_,j)=>({id:i*4+j}))}));
const local={viewMode:'grid',surface:'albums'};
const view={releaseWindow(){throw Error('unexpected release');},queueWindowReport:r=>captured=r};
const list={visible:true,contentY:1000,height:200,indexAt:(x,y)=>y<1100?10:11};
call('local/LocalAlbumCollection.qml','report',{root:local,view,list,nativeActive:true,nativeModel:{totalCount:40,rowAt:i=>entries[i]},width:800});
assert.equal(captured.length,24);
assert.equal(captured[0].id,40);
assert.deepEqual([...captured.map(x=>x.id)].sort((a,b)=>a-b),Array.from({length:24},(_,i)=>32+i));
// Scene must request the current band after row 48, not keep walking the head.
let urls;
const scene={isReady:true,visible:true,cardH:246,rowGap:11,rowModel:entries.map((e,i)=>({artists:[{artUrl:'u'+i}]})),requestCovers:u=>urls=u};
call('ArtistSceneView.qml','requestVisibleCovers',{root:scene},list);
assert.deepEqual([...urls],['u10','u11','u12','u13','u8','u9']);
console.log('Grid prefetch: two rows, visible priority, continuous-scroll flush, dedup and native ranges passed');

// Motion within the same band must not rebuild native QVariant rows or keep
// scheduling Library reports. Late data/layout/artwork callbacks bypass it.
let clock = 10000;
const fakeDate = {now: () => clock};
let reads = 0, sends = 0;
const motionCtx = {root:local,view:{releaseWindow(){},queueWindowReport(){sends++;}},
 list,nativeActive:true,nativeModel:{totalCount:40,rowAt:i=>{reads++;return entries[i];}},
 width:800,Date:fakeDate};
local._motionBand = '';
call('local/LocalAlbumCollection.qml','report',motionCtx,true);
for(let i=0;i<100;i++)call('local/LocalAlbumCollection.qml','report',motionCtx,true);
assert.equal(reads,6,'100 motion events in one band read native rows only once');
assert.equal(sends,1);
call('local/LocalAlbumCollection.qml','report',motionCtx);
assert.equal(sends,2,'late page/artwork reports bypass motion dedup');
clock += 1001;
call('local/LocalAlbumCollection.qml','report',motionCtx,true);
assert.equal(sends,3,'continued movement permits a retry refresh');
list.indexAt=(x,y)=>y<1100?11:12;
call('local/LocalAlbumCollection.qml','report',motionCtx,true);
assert.equal(sends,4,'crossing a row boundary immediately advances prefetch');
let gridSends=0;
const gridCtx={root,grid,Date:fakeDate,queueWindowReport(){gridSends++;}};
root._gridMotionBand='';
for(let i=0;i<100;i++)call('LibraryView.qml','gridWindowReport',gridCtx,true);
assert.equal(gridSends,1);
call('LibraryView.qml','gridWindowReport',gridCtx);
assert.equal(gridSends,2,'a filter/model replacement cannot be suppressed');
grid.contentY += grid.cellHeight;
call('LibraryView.qml','gridWindowReport',gridCtx,true);
assert.equal(gridSends,3);
clock += 1001;
call('LibraryView.qml','gridWindowReport',gridCtx,true);
assert.equal(gridSends,4);
console.log('Motion dedup: native reads, late pages, model refresh, row changes and retries passed');
