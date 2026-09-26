// DK-12 根因探针：loro-crdt doc.subscribe 在 import 时是否派发事件
import { LoroDoc } from "loro-crdt/nodejs";

const log = (m) => console.log(m);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// A：本地编辑 → export update
const A = new LoroDoc();
const mapA = A.getMap('doc');
mapA.set('p1', JSON.stringify({ type: 'paragraph', content: [{ type: 'text', text: 'hello from A' }] }));
await A.commit();
const updates = A.export({ mode: 'update' });
log(`A exported update: ${updates.length} bytes`);

// B：先订阅（模拟已 attach 的编辑器），后 import
const B = new LoroDoc();
let events = [];
B.subscribe((e) => { events.push({ by: e.by, containers: e.containers?.length ?? 0 }); });
const Bmap = B.getMap('doc');
Bmap.set('seed', '""');  // 建容器（与生产「空文档先建容器」语义对齐）
await B.commit();
events.length = 0;

B.import(updates);
log(`B imported, pending=${B.importStatus?.() ?? 'n/a'}`);
await sleep(1200);
log(`B doc.subscribe events after import: ${JSON.stringify(events)}`);
log(`B map.get(p1) = ${Bmap.get('p1')?.slice(0, 40) ?? 'MISSING'}`);

// 对照组：B 本地事务
events.length = 0;
Bmap.set('local', 'x');
await B.commit();
await sleep(600);
log(`B doc.subscribe events after LOCAL: ${JSON.stringify(events)}`);

// 对照组2：新 doc 验证（排除订阅时机）
const C = new LoroDoc();
let cEvents = [];
C.subscribe((e) => cEvents.push(e.by));
const Cmap = C.getMap('doc');
Cmap.set('seed', '""');
await C.commit();
C.import(updates);
await sleep(1200);
log(`C (fresh subscribe) events after import: ${JSON.stringify(cEvents)}`);
log(`C map.get(p1) = ${Cmap.get('p1')?.slice(0, 40) ?? 'MISSING'}`);
