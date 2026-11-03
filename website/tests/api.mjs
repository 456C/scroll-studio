import assert from 'node:assert/strict';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
const base=process.argv[2]||'http://127.0.0.1:4184';
let cookie='',csrf='';
async function call(action,{method='GET',project='',data,form,expected=200,other=false}={}){
 const headers={};if(!other&&cookie)headers.Cookie=cookie;if(method==='POST'&&!other)headers['X-CSRF-Token']=csrf;
 let body=form;if(data){body=JSON.stringify(data);headers['Content-Type']='application/json';}
 const response=await fetch(base+'/studio/api.php?action='+action+(project?'&project='+project:''),{method,headers,body});
 if(!other&&response.headers.get('set-cookie'))cookie=response.headers.get('set-cookie').split(';')[0];
 const bytes=Buffer.from(await response.arrayBuffer());let value;try{value=JSON.parse(bytes);}catch{value=bytes;}
 assert.equal(response.status,expected,JSON.stringify(value));return {value,response,bytes};
}
csrf=(await call('session')).value.csrf;
const created=(await call('create',{method:'POST'})).value;
const project=created.id;
await call('load',{project,other:true,expected:404});
await call('save',{project,method:'POST',other:true,data:{},expected:403});
const {profile,addEntity,addNote,pin,connect}=await import('../src/studio/model.js');
const story=created.story, chapters=structuredClone(story.chapters);
const char=addEntity(story,'characters','Ari'),loc=addEntity(story,'locations','Citadel'),group=addEntity(story,'groups','Watchers');
char.character.hair='Silver, shoulder length';group.character_ids=[char.id];group.location_ids=[loc.id];
const card=pin(story,char.id),note=addNote(story,[300,50]);story.lore[0].text='A secret oath';connect(story,card.id,note.id);
story.chapters[0].name='SHOULD NOT REPLACE CHAPTER';
let result=(await call('save',{method:'POST',project,data:{story,revision:0}})).value;
const loaded=(await call('load',{project})).value;assert.deepEqual(loaded.story.chapters,chapters);
await call('save',{method:'POST',project,data:{story,revision:0},expected:409});
const fork=(await call('fork',{method:'POST',project,data:{story,revision:0}})).value;
assert.equal(fork.story.lore[0].text,'A secret oath');assert.notEqual(fork.id,project);
assert.deepEqual(fork.story.chapters,chapters);
const form=new FormData();form.append('revision','0');form.append('csrf',csrf);form.append('project',fork.id);
const exported=await call('export',{method:'POST',project:fork.id,form});
assert.equal(exported.bytes.subarray(0,2).toString(),'PK');
await mkdir('target/web-tests',{recursive:true});await writeFile('target/web-tests/browser-created.scro',exported.bytes);
const importForm=new FormData();importForm.append('file',new Blob([await readFile('target/world-review/World review.scro')]),'World.scro');
const imported=(await call('import',{method:'POST',form:importForm})).value;
assert.ok(imported.story.assets.length>0);
const image=imported.story.assets.find(a=>a.width>0);
assert.ok(image);
const original=await fetch(base+'/studio/api.php?action=image&project='+imported.id+'&id='+image.id+'&size=original',{headers:{Cookie:cookie}});
assert.equal(original.status,200);assert.ok(original.headers.get('content-type').startsWith('image/'));
const preview=await fetch(base+'/studio/api.php?action=image&project='+imported.id+'&id='+image.id+'&size=384',{headers:{Cookie:cookie}});
assert.equal(preview.status,200);assert.equal(preview.headers.get('content-type'),'image/webp');
const uf=new FormData();uf.append('file',new Blob([await original.arrayBuffer()],{type:'image/png'}),'new.png');uf.append('revision',0);uf.append('kind','Background');
const uploaded=(await call('upload',{method:'POST',project:imported.id,form:uf})).value;assert.equal(uploaded.asset.kind,'Background');
const withImage=(await call('load',{project:imported.id})).value;
const folder=withImage.story.locations[0];folder.asset_ids.push(uploaded.asset.id);
withImage.story.card_thumbnails=[{target:folder.id,asset_id:uploaded.asset.id}];
const coverSaved=(await call('save',{method:'POST',project:imported.id,data:withImage})).value;
const coverForm=new FormData();coverForm.append('csrf',csrf);coverForm.append('revision',coverSaved.revision);coverForm.append('project',imported.id);
const coverExport=await call('export',{method:'POST',project:imported.id,form:coverForm});
await writeFile('target/web-tests/browser-thumbnail.scro',coverExport.bytes);
const reopenedForm=new FormData();reopenedForm.append('file',new Blob([coverExport.bytes]),'cover.scro');
const reopened=(await call('import',{method:'POST',form:reopenedForm})).value;
assert.deepEqual(reopened.story.card_thumbnails,withImage.story.card_thumbnails);
const bad=structuredClone((await call('load',{project:imported.id})).value);
bad.story.assets.find(a=>a.width>0).width++;await call('save',{method:'POST',project:imported.id,data:bad,expected:422});
console.log('API passed: session isolation, CSRF, original preservation, world edits, lore, revision conflict, recovery copy, upload, previews, export.');

