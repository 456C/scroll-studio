import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
const base='https://scroll.project456.net';
for(const path of ['/','/studio/','/studio/app.js','/studio/studio.css','/studio/board.js','/studio/timeline.js','/studio/editing.js','/studio/model.js','/assets/Karla.ttf']){
 const r=await fetch(base+path);assert.equal(r.status,200,path);
 const text=await r.text();assert.ok(text.length>100,path);
 if(path==='/')assert.ok(text.includes('href="/studio/"'));
 if(path==='/studio/')assert.ok(text.includes('Assets — Scroll Studio'));
 if(path==='/studio/app.js')assert.ok(text.includes('Use as thumbnail'));
}
for(const path of ['/studio/runtime.php','/studio/template.json','/studio/test.php','/studio/api.php/anything','/missing-file']){
 assert.equal((await fetch(base+path)).status,404,path);
}
const session=await fetch(base+'/studio/api.php?action=session');
const cookie=session.headers.get('set-cookie');assert.ok(cookie.includes('secure'));assert.ok(cookie.includes('HttpOnly'));assert.ok(cookie.includes('SameSite=Strict'));
assert.ok(session.headers.get('content-security-policy').includes("frame-ancestors 'self'"));
const release=await fetch(base+'/release.json').then(r=>r.json());assert.equal(release.version,(await readFile('Cargo.toml','utf8')).match(/^version = "([\d.]+)"/m)[1]);
for(const name of [release.installer,release.portable]){
 const remote=Buffer.from(await fetch(base+'/downloads/'+name).then(r=>r.arrayBuffer()));
 const local=await readFile('dist/'+name);assert.equal(createHash('sha256').update(remote).digest('hex'),createHash('sha256').update(local).digest('hex'));
}
const redirect=await fetch('http://scroll.project456.net/studio/',{redirect:'manual'});assert.equal(redirect.status,301);assert.equal(redirect.headers.get('location'),base+'/studio/');
console.log('Live HTTPS verified: Assets navigation, PHP routes, protected helpers, session cookie flags, redirects, and both complete Windows download hashes.');

