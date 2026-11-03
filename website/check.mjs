import assert from 'node:assert/strict';
import {readFile, stat, readdir} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
const root=path.join(path.dirname(fileURLToPath(import.meta.url)),'dist');
const release=JSON.parse(await readFile(path.join(root,'release.json'),'utf8'));
const pages=(await readdir(root)).filter(name=>name.endsWith('.html'));
for(const page of pages){
  const html=await readFile(path.join(root,page),'utf8');
  assert(!/\{\{\w+\}\}/.test(html),`${page}: unresolved template`);
  assert(/name="viewport"/.test(html),`${page}: missing viewport`);
  const ids=[...html.matchAll(/\bid="([^"]+)"/g)].map(m=>m[1]);
  assert.equal(ids.length,new Set(ids).size,`${page}: duplicate IDs`);
  for(const [,attribute,url]of html.matchAll(/\b(href|src)="([^"]+)"/g)){
    if (attribute === 'href' && url === 'https://ko-fi.com/456th') continue;
    assert(!url.startsWith('http'),`${page}: unexpected external dependency or link ${url}`);
    const [file,fragment]=url.split('#');
    if(file){
      const relative=decodeURIComponent(file).replace(/^\/+/, '') || 'index.html';
      const resolved=path.resolve(root,relative);
      assert(resolved.startsWith(root+path.sep),'Link escapes public build');
      assert((await stat(resolved)).isFile(),`${page}: missing ${file}`);
    }else if(fragment)assert(ids.includes(fragment),`${page}: missing anchor ${fragment}`);
  }
}
for(const filename of [release.installer,release.portable]){
  const content=await readFile(path.join(root,'downloads',filename));
  const expected=(await readFile(path.join(root,'downloads',filename+'.sha256'),'utf8')).split(/\s/)[0];
  const actual=createHash('sha256').update(content).digest('hex');
  assert.equal(actual,expected,`${filename}: checksum mismatch`);
  assert(content.length<25*1024*1024,'Static hosting limit exceeded');
  if(filename===release.installer){assert.equal(actual,release.sha256);assert.equal(content.length,release.bytes);}
}
const index=await readFile(path.join(root,'index.html'),'utf8');
assert(index.includes(`Version ${release.version}`),'Page version differs from downloads');
assert.equal((index.match(/role="tab"/g)||[]).length,3);
assert.equal((index.match(/role="tabpanel"/g)||[]).length,3);
const script=await readFile(path.join(root,'app.js'),'utf8');
assert(!/fetch\(|localStorage|sessionStorage|document\.cookie/.test(script),'Unexpected tracking or storage');
console.log(`Verified ${pages.length} pages, local links, tab wiring and both ${release.version} download hashes.`);
