import {icon,escape as esc} from './icons.js';
import * as M from './model.js';
import {LoreBoard} from './board.js';
import {showAngle} from './camera.js';
import {TimelineBoard,wouldMove} from './timeline.js';
import {focusForTyping,prepareEditors,installKeyboardDismissal} from './editing.js';
installKeyboardDismissal();

const $=(s,root=document)=>root.querySelector(s), $$=(s,root=document)=>[...root.querySelectorAll(s)];
const app=$('#app'), overlay=$('#overlay');
const mobile=()=>matchMedia('(max-width:680px)').matches;
const sections=[['characters','Characters'],['locations','Locations'],['groups','Groups'],['timeline','Timeline'],['lore','Lore'],['images','Images'],['settings','Settings']];
let csrf='',recent=[],project=null,story=null,revision=0,serial=0,savedSerial=0,saveTimer=null,saving=null,saveError=null;
let section='characters',selected=null,sheetTab='References',subfield=null,query='',imageFilter='All',galleryLimit=60,board=null,boardView={x:0,y:0,zoom:1,ready:false,selected:null,edge:null},editKey=null;
let history=new M.History(mobile()?12*1024*1024:32*1024*1024),imageHandler=null,toastTimer=null,menuElement=null,menuAbort=null,modalCleanup=null,dbPromise=null;
let draftTimer=null,galleryPosition=0,timelineBoard=null,timelineView={selected:null,scroll:0};
let appearance={glass:true,motion:true};
try{appearance={...appearance,...JSON.parse(localStorage.getItem('scroll-appearance')||'{}')};}catch{}
applyAppearance();
function applyAppearance(){document.body.classList.toggle('no-glass',!appearance.glass);document.body.classList.toggle('no-motion',!appearance.motion);}
function button(label,action,ic='',extra=''){return '<button type="button" data-action="'+action+'" '+extra+'>'+(ic?icon(ic):'')+(label?'<span>'+esc(label)+'</span>':'')+'</button>';}
function ib(ic,action,label){return button('',action,ic,'class="icon ghost" title="'+esc(label)+'" aria-label="'+esc(label)+'"');}
function rowEmpty(title,kind,action,label){return '<div class="empty"><div class="empty-inner"><div class="empty-mark">'+icon(kind)+'</div><h2>'+esc(title)+'</h2>'+button(label,action,'plus','class="primary"')+'</div></div>';}
function imageURL(id,size='384',download=false){return '/studio/api.php?action=image&project='+project+'&id='+id+'&size='+size+(download?'&download=1':'');}
function imageTag(a,size='384'){return '<img src="'+imageURL(a.id,size)+'" alt="'+esc(a.name)+'" width="'+a.width+'" height="'+a.height+'" loading="lazy" decoding="async">';}
function asset(id){return story?.assets.find(a=>a.id===Number(id));}
function entry(){return section==='characters'?asset(selected):story?.[section]?.find(e=>e.id===selected);}
function toast(message){const t=$('#toast');t.textContent=message;t.classList.add('show');clearTimeout(toastTimer);toastTimer=setTimeout(()=>t.classList.remove('show'),4200);}
async function api(action,{method='GET',data=null,form=null,id=project}={}){
  const options={method,credentials:'same-origin',headers:{}};
  if(method==='POST'){options.headers['X-CSRF-Token']=csrf;if(data){options.headers['Content-Type']='application/json';options.body=JSON.stringify(data);}if(form)options.body=form;}
  const response=await fetch('/studio/api.php?action='+action+(id?'&project='+id:''),options);
  let result;try{result=await response.json();}catch{throw Error(response.status===413?'This file is larger than the server upload limit.':'The server did not return a valid response. Try again.');}
  if(!response.ok){const e=Error(result.error||'Request failed.');e.status=response.status;throw e;}return result;
}
function database(){
  return dbPromise??=new Promise((resolve,reject)=>{const r=indexedDB.open('scroll-assets-draft',1);r.onupgradeneeded=()=>r.result.createObjectStore('draft');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
}
async function draftRead(){try{const db=await database();return await new Promise(r=>{const q=db.transaction('draft').objectStore('draft').get('current');q.onsuccess=()=>r(q.result);q.onerror=()=>r(null);});}catch{return null;}}
async function draftWrite(){if(!story)return;try{const value={project,story:M.clone(story),revision,dirty:serial!==savedSerial,time:Date.now()};const db=await database();await new Promise((resolve,reject)=>{const tx=db.transaction('draft','readwrite');tx.objectStore('draft').put(value,'current');tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);});}catch{ /* Server save and explicit export remain available when storage is disabled. */ }}
function checkpoint(){history.push(story);editKey=null;}
function status(){
  const dot=$('.save-dot');if(!dot)return;
  const dirty=serial!==savedSerial;
  dot.className='save-dot'+(saveError?' failed':dirty?' dirty':'');
  dot.title=saveError?saveError.message:dirty?'Saving temporary workspace…':'Saved temporarily · Export .scro to keep';
  dot.setAttribute('aria-label',dot.title);
  const title=$('.world-title');if(title)title.textContent=story.title||'Untitled world';
  const undo=$('[data-action="undo"]'),redo=$('[data-action="redo"]');if(undo)undo.disabled=!history.undo.length;if(redo)redo.disabled=!history.redo.length;
}
function changed(refresh=false){M.pruneThumbnails(story);serial++;saveError=null;status();clearTimeout(saveTimer);saveTimer=setTimeout(()=>flush().catch(reportSaveError),700);clearTimeout(draftTimer);draftTimer=setTimeout(draftWrite,250);if(refresh)renderWorkspace();else if(section==='lore')board?.refresh();}
function reportSaveError(error){saveError=error;status();toast(error.status===409?'Another tab saved changes. Open File → Save status to keep your copy.':error.message);}
async function flush(){
  clearTimeout(saveTimer);
  if(saving){await saving;if(serial!==savedSerial)return flush();return;}
  if(!story||serial===savedSerial)return;
  const value=M.clone(story),savedAt=serial,expected=revision,currentProject=project;
  saving=api('save',{method:'POST',data:{story:value,revision:expected},id:currentProject}).then(result=>{
    if(project===currentProject){revision=result.revision;story.next_id=Math.max(story.next_id,result.next_id);savedSerial=savedAt;saveError=null;status();draftWrite();}
  }).finally(()=>{saving=null;});
  await saving;
  if(serial!==savedSerial)return flush();
}
async function busy(label,task){
  const mask=document.createElement('div');mask.className='busy-mask';mask.innerHTML='<div class="busy-card"><div class="spinner"></div><div><div class="busy-label">'+esc(label)+'</div><progress hidden max="100"></progress></div></div>';document.body.append(mask);
  try{return await task((value)=>{const p=$('progress',mask);p.hidden=false;p.value=value;});}catch(e){toast(e.message);throw e;}finally{mask.remove();}
}
function adopt(result,writeDraft=true){
  project=result.id;story=result.story;revision=result.revision;serial=0;savedSerial=0;saveError=null;history=new M.History(mobile()?12*1024*1024:32*1024*1024);
  selected=null;section='characters';sheetTab='References';query='';timelineView={selected:null,scroll:0};boardView={x:0,y:0,zoom:1,ready:false,selected:null,edge:null};render();if(writeDraft)draftWrite();
}
async function openWorld(id){await flush();await busy('Opening world…',async()=>{const data=await api('load',{id});adopt(data);});}
async function newWorld(){await flush();await busy('Creating world…',async()=>adopt(await api('create',{method:'POST',id:null})));}
async function importWorld(file){
  if(!file)return;await flush();if(file.size>536870912)throw Error('A .scro file can be up to 512 MiB.');
  await busy('Opening '+file.name+'…',progress=>new Promise((resolve,reject)=>{
    const form=new FormData();form.append('file',file);
    const xhr=new XMLHttpRequest();xhr.open('POST','/studio/api.php?action=import');xhr.setRequestHeader('X-CSRF-Token',csrf);
    xhr.upload.onprogress=e=>{if(e.lengthComputable)progress(Math.round(e.loaded/e.total*100));};
    xhr.onload=()=>{try{const result=JSON.parse(xhr.responseText);if(xhr.status!==200)throw Error(result.error||'Import failed.');adopt(result);resolve();}catch(e){reject(e);}};
    xhr.onerror=()=>reject(Error('Connection interrupted. Your original .scro file is unchanged.'));xhr.send(form);
  }));
}
async function exportWorld(){
  await busy('Preparing your .scro…',async()=>{
    await flush();
    const form=document.createElement('form');form.method='post';form.action='/studio/api.php?action=export';form.target='download-frame';
    for(const [key,value]of Object.entries({csrf,project,revision})){const input=document.createElement('input');input.type='hidden';input.name=key;input.value=value;form.append(input);}
    document.body.append(form);form.submit();form.remove();
  });
  toast('Download requested · Keep the .scro file for desktop use.');
}
function shell(){
  return '<header class="topbar">'+button('File','file')+button('New','newMenu','plus','class="new-top"')+'<span class="world-title">'+esc(story?.title||'Scroll Studio')+'</span>'+(story?'<button class="save-dot" data-action="saveStatus" aria-label="Save status"></button>'+button('Style','style','style','class="top-style"')+ib('copy','copyStyle','Copy story style')+button('Export .scro','export','export','class="primary export-button" aria-label="Export .scro"'):'<a href="/" class="muted">Download app ↗</a>')+'</header>';
}
function render(){
  closeMenu();board?.destroy();board=null;timelineBoard?.destroy();timelineBoard=null;
  if(!story){app.innerHTML=shell()+'<main class="welcome"><div class="welcome-layout'+(!recent.length?' solo':'')+'"><div class="welcome-main"><img src="/assets/app-icon.png" width="64" height="64" alt=""><h1>Your world, anywhere.</h1>'+button('Open .scro','open','open','class="primary"')+button('New world','newWorld','plus')+'<small>Temporary workspace · Export .scro to keep<br><a href="/privacy.html">How your files are handled</a></small></div>'+(recent.length?'<div class="welcome-recent"><small>RECENT WORKSPACES</small>'+recent.map(r=>button(r.title,'recent:'+r.id,'open')).join('')+'</div>':'')+'</div></main>';return;}
  app.innerHTML=shell()+'<aside class="sidebar"><nav class="nav-list" aria-label="World sections">'+sections.map(([key,label])=>button(label,'nav:'+key,key,'class="'+(section===key?'active':'')+'"')).join('')+'</nav><div id="inspector"></div><div class="sidebar-footer">'+ib('undo','undo','Undo')+ib('redo','redo','Redo')+'</div></aside><main class="workspace" id="workspace"></main><nav class="mobile-nav" aria-label="World sections">'+[['characters','Characters'],['locations','Locations'],['lore','Lore'],['more','More']].map(([key,label])=>button(label,key==='more'?'mobileMenu':'nav:'+key,key,'class="'+(section===key?'active':'')+'"')).join('')+'</nav>';
  renderWorkspace();status();
}
function navigate(key){galleryPosition=0;section=key;selected=null;sheetTab='References';subfield=null;query='';galleryLimit=60;editKey=null;render();}
function renderWorkspace(){
  board?.destroy();board=null;timelineBoard?.destroy();timelineBoard=null;const root=$('#workspace');if(!root)return;
  const side=$('#inspector');if(side)side.innerHTML='';
  if(section==='lore'){renderLore(root);return;}
  if(section==='timeline'){renderTimeline(root);return;}
  if(section==='settings'){renderSettings(root);return;}
  if(selected && entry()){renderSheet(root);return;}
  renderGallery(root);
}
function renderGallery(root){
  const type=section,label=sections.find(([k])=>k===type)?.[1]||'Images';
  root.innerHTML='<div class="workspace-header"><h1>'+label+'</h1><input type="search" class="search" data-search placeholder="Search" aria-label="Search '+label+'" value="'+esc(query)+'">'+button(type==='images'?'Import':type==='locations'?'Location':type==='groups'?'Group':'Character',type==='images'?'importLibrary':'addEntity','plus','class="primary"')+'</div>'+(type==='images'?'<div class="section-tabs" style="padding:0 24px 14px">'+['All','Backgrounds','References'].map(v=>button(v,'filter:'+v,'','class="'+(imageFilter===v?'active':'')+'"')).join('')+'</div>':'')+'<div class="content-scroll drop-zone" id="gallery-content"></div>';
  const content=$('#gallery-content');
  const paint=()=>{
  let items=type==='characters'?story.assets.filter(a=>a.kind==='Character'):type==='images'?story.assets.filter(a=>a.width>0):story[type]||[];
  if(type==='images' && imageFilter!=='All')items=items.filter(a=>a.kind===(imageFilter==='Backgrounds'?'Background':'Image'));
  if(query)items=items.filter(a=>(a.name+' '+(a.character?.tagline||a.profile?.description||a.description||'')).toLowerCase().includes(query.toLowerCase()));
  if(!items.length){content.innerHTML=rowEmpty(query?'No matches':label,type,type==='images'?'importLibrary':'addEntity',type==='images'?'Import image':'Add '+label.slice(0,-1));}
  else content.innerHTML='<div class="gallery">'+items.slice(0,galleryLimit).map(e=>{
    const refs=type==='images'?[e]:M.references(story,type,e),art=asset(M.thumbnail(story,e.id,refs[0]?.id));
    const groups=story.groups.filter(g=>type==='characters'?g.character_ids.includes(e.id):type==='locations'?g.location_ids.includes(e.id):false);
    const subtitle=type==='characters'?e.character.tagline:type==='locations'?e.profile.description:type==='groups'?e.description:e.width+' × '+e.height;
    return '<button class="entity-card" data-action="'+(type==='images'?'view:':'select:')+e.id+'"><div class="card-art">'+(art?imageTag(art):icon(type))+'</div><div class="card-info"><h3>'+esc(e.name)+'</h3><p>'+esc(subtitle||refs.length+' reference'+(refs.length===1?'':'s'))+'</p>'+(groups.length?'<div class="tags">'+groups.slice(0,2).map(g=>'<span class="tag">'+esc(g.name)+'</span>').join('')+'</div>':'')+'</div></button>';
  }).join('')+'</div>'+(items.length>galleryLimit?button('Load more','loadMore','','class="load-more"'):'');
  };
  paint();
  $('[data-search]',root)?.addEventListener('input',e=>{query=e.target.value;galleryLimit=60;galleryPosition=0;paint();content.scrollTop=0;});
  content.scrollTop=galleryPosition;
  bindDrop(content,files=>uploadImages(files,null));
}
function renderSheet(root){
  const e=entry(),type=section;
  const tabs=type==='characters'?['References','Appearance','Outfits','Palette','Identity','Notes','Prompt']:type==='locations'?['References','Details','Notes','Prompt']:['References','Identity','Members'];
  if(!tabs.includes(sheetTab))sheetTab=tabs[0];
  root.innerHTML='<section class="sheet"><div class="sheet-header">'+ib('back','backGallery','Back to gallery')+'<h1 class="sheet-name">'+esc(e.name)+'</h1><div class="sheet-actions">'+(type!=='groups'?ib('groups','affiliations','Affiliations'):'')+button('Copy prompt','copyPrompt','copy','class="secondary-sheet-action"')+(type==='characters'?ib('angle','angle','Character angle'):'')+ib('more','entityMenu','More options')+'</div></div><div class="sheet-body"><nav class="sheet-tabs" aria-label="Sheet categories">'+tabs.map(t=>button(t,'sheet:'+t,'','class="'+(sheetTab===t?'active':'')+'"')).join('')+'</nav><div class="sheet-content drop-zone" id="sheet-content"></div></div></section>';
  const content=$('#sheet-content');
  content.addEventListener('contextmenu',event=>{const image=event.target.closest('[data-action^="view:"]');if(image){event.preventDefault();referenceMenu(Number(image.dataset.action.split(':')[1]),{x:event.clientX,y:event.clientY});}});
  if(sheetTab==='References')renderReferences(content,e,type);
  else if(sheetTab==='Appearance')renderFocused(content,e.character,[['face','Face'],['hair','Hair'],['eyes','Eyes'],['skin','Skin'],['build','Build'],['distinguishing_features','Features'],['consistency','Consistency']],'character.');
  else if(sheetTab==='Identity' && type==='characters'){
    content.innerHTML='<div class="field"><label>Gender</label><div class="row">'+['Female','Male'].map(g=>button(g,'gender:'+g,'','class="'+(e.character.gender===g?'active':'')+'"')).join('')+'</div></div><div id="identity-editor" class="focused-editor"></div>';
    renderFocused($('#identity-editor'),e.character,[['tagline','Concept'],['role','Role'],['age','Age'],['personality','Personality'],['backstory','Backstory'],['relationships','Relationships']],'character.');
  } else if(sheetTab==='Identity'){
    content.innerHTML='<div class="row"><label for="group-color">Group color</label><input type="color" id="group-color" value="'+M.hex(e.color)+'" data-group-color></div><div class="focused-editor"><label class="muted">Identity</label><textarea data-bind="description" aria-label="Group description" maxlength="5000" placeholder="What defines this group?">'+esc(e.description)+'</textarea></div>';
    $('[data-group-color]').addEventListener('input',ev=>{fieldCheckpoint('group-color');e.color=M.rgb(ev.target.value);changed();});
  } else if(sheetTab==='Details')renderFocused(content,e.profile,[['description','Description'],['atmosphere','Atmosphere'],['lighting','Lighting'],['consistency','Consistency']],'profile.');
  else if(sheetTab==='Notes')renderFocused(content,type==='characters'?e:e.profile,[['notes','Notes'],...(type==='characters'?[['character.backstory','Backstory']]:[])],type==='characters'?'':'profile.');
  else if(sheetTab==='Outfits')renderOutfits(content,e);
  else if(sheetTab==='Palette')renderPalette(content,e);
  else if(sheetTab==='Members')renderMembers(content,e);
  else if(sheetTab==='Prompt')renderPrompt(content,e,type);
  bindFields(root,e);prepareEditors(content);bindDrop(content,files=>uploadImages(files,{type,id:e.id}));
}
function getPath(object,path){return path.split('.').reduce((a,k)=>a?.[k],object);}
function setPath(object,path,value){const keys=path.split('.');let target=object;for(const k of keys.slice(0,-1))target=target[k];target[keys.at(-1)]=value;}
function fieldCheckpoint(key){if(editKey!==key){checkpoint();editKey=key;}}
function bindFields(root,object){
  $$('[data-bind]',root).forEach(input=>{
    input.addEventListener('input',()=>{fieldCheckpoint((selected||'story')+':'+input.dataset.bind);setPath(object,input.dataset.bind,input.value);changed();});
    input.addEventListener('blur',()=>{editKey=null;});
  });
}
function renderFocused(root,object,fields,prefix){
  const key=fields.some(([k])=>k===subfield)?subfield:fields[0][0];
  root.innerHTML='<div class="section-tabs" aria-label="Detail fields">'+fields.map(([k,label])=>button(label,'subfield:'+k,'','class="'+(key===k?'active':'')+'"')).join('')+'</div><div class="focused-editor"><textarea data-bind="'+prefix+key+'" aria-label="'+esc(fields.find(([k])=>k===key)[1])+'" maxlength="'+(section==='groups'?5000:250000)+'" placeholder="'+esc(fields.find(([k])=>k===key)[1])+'…">'+esc(getPath(object,key)||'')+'</textarea></div>';
}
function renderReferences(root,e,type){
  const refs=M.references(story,type,e);
  if(!refs.length){root.innerHTML='<div class="empty"><div class="empty-inner"><div class="empty-mark">'+icon('images')+'</div><div class="row wrap">'+button('Add image','addReference','plus','class="primary"')+button('From library','pickReference','images')+'</div></div></div>';return;}
  root.innerHTML='<div class="row wrap">'+button('Add image','addReference','plus')+button('From library','pickReference','images')+button('Export sheet','exportSheet','export')+'<span class="grow"></span><small>'+refs.length+' references</small></div>'+(refs.length?'<div class="masonry">'+refs.map(a=>'<article class="reference-card"><button class="reference-open" data-action="view:'+a.id+'" aria-label="View '+esc(a.label||a.name)+'">'+imageTag(a,mobile()?'384':'768')+'</button><div class="reference-bottom"><span>'+esc(a.label||a.name)+'</span>'+ib('more','refMenu:'+a.id,'Reference options')+'</div></article>').join('')+'</div>':rowEmpty('References','images','addReference','Add image'));
}
function renderOutfits(root,e){
  const p=e.character;let index=Number(subfield);if(!Number.isInteger(index)||!p.outfits[index])index=p.active_outfit??0;
  root.innerHTML='<div class="row">'+button('Outfit','addOutfit','plus')+'<span class="grow"></span>'+(p.outfits[index]?button(p.active_outfit===index?'Active outfit':'Use in prompts','activeOutfit:'+index,p.active_outfit===index?'check':''):'')+'</div>';
  if(!p.outfits.length){root.innerHTML+=rowEmpty('Outfits','characters','addOutfit','Add outfit');return;}
  root.innerHTML+='<div class="outfit-layout"><div class="outfit-list">'+p.outfits.map((o,i)=>button(o.name||'Outfit '+(i+1),'subfield:'+i,'','class="'+(index===i?'active':'')+'"')).join('')+'</div><div class="outfit-details"><input aria-label="Outfit name" maxlength="1024" data-bind="character.outfits.'+index+'.name" value="'+esc(p.outfits[index].name)+'"><textarea aria-label="Outfit description" maxlength="250000" data-bind="character.outfits.'+index+'.description" placeholder="Materials, silhouette, accessories…">'+esc(p.outfits[index].description)+'</textarea>'+button('Remove outfit','removeOutfit:'+index,'trash','class="ghost danger"')+'</div></div>';
}
function renderPalette(root,e){
  root.innerHTML='<div class="row">'+button('Color','addColor','plus')+'<span class="grow"></span><small>'+e.character.palette.length+' / 16</small></div><div class="palette">'+e.character.palette.map((s,i)=>'<div><div class="swatch"><input type="color" aria-label="Palette color '+(i+1)+'" value="'+M.hex(s.color)+'" data-palette="'+i+'"></div>'+button('Remove','removeColor:'+i,'','class="ghost"')+'</div>').join('')+'</div>';
  $$('[data-palette]',root).forEach(input=>input.addEventListener('input',()=>{const i=Number(input.dataset.palette);fieldCheckpoint('palette:'+i);e.character.palette[i].color=[...M.rgb(input.value),e.character.palette[i].color[3]];changed();}));
}
function renderMembers(root,e){
  root.innerHTML='<div class="section-tabs">'+['Characters','Locations'].map(t=>button(t,'subfield:'+t,'','class="'+((subfield||'Characters')===t?'active':'')+'"')).join('')+'</div><div class="member-grid"></div>';
  const chars=subfield!=='Locations',items=chars?story.assets.filter(a=>a.kind==='Character'):story.locations,key=chars?'character_ids':'location_ids';
  $('.member-grid',root).innerHTML=items.length?items.map(a=>'<label class="check-row"><input type="checkbox" data-member="'+a.id+'" '+(e[key].includes(a.id)?'checked':'')+'><span>'+esc(a.name)+'</span></label>').join(''):'<p class="muted">No '+(chars?'characters':'locations')+' yet.</p>';
  $$('[data-member]',root).forEach(input=>input.addEventListener('change',()=>{checkpoint();e[key]=e[key].filter(id=>id!==Number(input.dataset.member));if(input.checked)e[key].push(Number(input.dataset.member));changed();}));
}
function renderPrompt(root,e,type){
  const key=type==='characters'?'prompt':'profile.prompt',avoid=type==='characters'?'character.avoid':'profile.avoid';
  root.innerHTML='<div class="row wrap">'+(type==='characters'?'<select id="prompt-kind" aria-label="Image request">'+['Portrait','Turnaround','Expressions','Scene'].map(k=>'<option>'+k+'</option>').join(''):'')+button('Angle','angle','angle')+button('Copy prompt','copyPrompt','copy','class="primary"')+'</div><div class="field-grid"><div class="field"><label>Direction</label><textarea rows="3" data-bind="'+key+'" aria-label="Additional prompt direction" maxlength="250000">'+esc(getPath(e,key))+'</textarea></div><div class="field"><label>Avoid</label><textarea rows="3" data-bind="'+avoid+'" aria-label="Avoid in generations" maxlength="250000">'+esc(getPath(e,avoid))+'</textarea></div></div><textarea id="scene-direction" aria-label="Scene and camera direction" rows="2" placeholder="Scene / camera direction…"></textarea><div class="prompt-preview" id="prompt-preview"></div>';
  const update=()=>{$('#prompt-preview').textContent=M.prompt(story,type,e,$('#prompt-kind')?.value||'Scene',$('#scene-direction').value);};
  root.addEventListener('input',()=>queueMicrotask(update));$('#prompt-kind')?.addEventListener('change',update);update();
}
function currentPrompt(){return M.prompt(story,section,entry(),$('#prompt-kind')?.value||'Portrait',$('#scene-direction')?.value||'');}
function renderTimeline(root){
  timelineBoard=new TimelineBoard(root,{entries:M.timeline(story),chapters:story.chapters,view:timelineView,
    move:(id,before)=>{
      if(!wouldMove(M.timeline(story).map(e=>e.chapter_id),id,before))return;
      checkpoint();M.reorderTimeline(story,id,before);changed(true);
      const card=root.querySelector('[data-chapter="'+id+'"] .chapter-card');
      card?.focus({preventScroll:true});card?.scrollIntoView({block:'nearest',inline:'nearest'});
      const message=root.querySelector('[role=status]');if(message)message.textContent='Moved to position '+(M.timeline(story).findIndex(e=>e.chapter_id===id)+1);
    },edit:id=>{
      const e=M.timeline(story).find(e=>e.chapter_id===id);
      modal('Timeline notes','<label class="field">When<input id="timeline-when" maxlength="1024" value="'+esc(e.when)+'" placeholder="Date or era"></label><label class="field">Notes<textarea id="timeline-summary" maxlength="20000" rows="6">'+esc(e.summary)+'</textarea></label>',button('Cancel','closeModal')+button('Save','saveTimeline','','class="primary"'));
      $('[data-action="saveTimeline"]').onclick=()=>{
        const when=$('#timeline-when').value,summary=$('#timeline-summary').value;closeModal();
        if(when===e.when&&summary===e.summary)return;
        checkpoint();story.timeline=M.timeline(story);Object.assign(story.timeline.find(t=>t.chapter_id===id),{when,summary});changed(true);
      };
    }
  });
}
function renderSettings(root){
  root.innerHTML='<div class="workspace-header"><h1>Settings</h1></div><div class="content-scroll"><div class="settings-list"><div class="setting"><div>Frosted surfaces<small>Controls and menus</small></div><input type="checkbox" data-setting="glass" aria-label="Frosted surfaces" '+(appearance.glass?'checked':'')+'></div><div class="setting"><div>Motion<small>Brief transitions</small></div><input type="checkbox" data-setting="motion" aria-label="Motion" '+(appearance.motion?'checked':'')+'></div><div class="setting"><div>Original artwork<small>Previews resize. Original files stay intact.</small></div>'+icon('check')+'</div><div class="setting"><div>Temporary workspace<small>Expires after 24 hours without use.</small></div>'+button('Export','export','export')+'</div><p class="plain-text">Edits are saved temporarily on this server and a recovery draft is kept in this browser. Export .scro to keep your world and open it in Scroll Studio 0.13.1 or newer. <a href="/privacy.html">Privacy</a></p>'+button('Clear browser recovery draft','clearDraft','trash','class="ghost"')+'</div></div>';
  $$('[data-setting]',root).forEach(input=>input.addEventListener('change',()=>{appearance[input.dataset.setting]=input.checked;localStorage.setItem('scroll-appearance',JSON.stringify(appearance));applyAppearance();}));
}
function renderLore(root){
  root.innerHTML='<div class="board-header"><h1>Lore</h1>'+button('Note','addNote','plus')+button('Card','pinCard','characters')+ib('link','connectMode','Connect cards')+'<span class="spacer"></span><span class="board-zoom">100%</span>'+ib('fit','fitBoard','Fit board')+'</div><div class="board" id="lore-board" aria-label="Lore investigation board" tabindex="0"></div>';
  board=new LoreBoard($('#lore-board'),{
    story:()=>story,view:boardView,imageURL,checkpoint,changed:()=>changed(),inspect:()=>inspectLore(),
    open:id=>openTarget(id),menu:(items,x,y)=>menu(items,{x,y}),addNote:position=>createBoardNote(position),pin:position=>pinPicker(position),remove:id=>removeCard(id)
  });
  board.refresh();inspectLore(false);
}
function openTarget(id){const t=M.targets(story).get(id);if(!t)return;if(t.type==='note'){boardView.selected=story.lore_board.cards.find(c=>c.target===id)?.id;inspectLore();return;}section=t.type;selected=id;sheetTab='References';subfield=null;render();}
function inspectLore(openMobile=true){
  const card=story.lore_board.cards.find(c=>c.id===boardView.selected),edge=story.lore_board.connections.find(e=>e.id===boardView.edge),target=card?M.targets(story).get(card.target):null;
  let html='';
  if(edge)html='<small>CONNECTION</small><input data-lore="label" aria-label="Relationship" maxlength="1024" value="'+esc(edge.label)+'" placeholder="Relationship"><input type="color" data-lore-color value="'+M.hex(edge.color)+'" aria-label="Connection color">'+button('Remove connection','removeEdge','trash','class="ghost danger"');
  else if(card&&target)html='<small>'+esc(target.label.toUpperCase())+'</small>'+(target.type==='note'?'<input data-lore="title" aria-label="Note title" maxlength="1024" value="'+esc(target.name)+'">':'<h3>'+esc(target.name)+'</h3>'+button('Open sheet','openTarget:'+card.target,'open'))+'<div class="row"><input type="color" data-lore-color value="'+M.hex(card.color)+'" aria-label="Card color">'+button('Connect','connectMode','link')+'</div><textarea data-lore="'+(target.type==='note'?'text':'note')+'" aria-label="'+(target.type==='note'?'Note':'Board note')+'" maxlength="25000" placeholder="Add a detail…">'+esc(target.type==='note'?target.text:card.note)+'</textarea>'+button(target.type==='note'?'Delete note':'Unpin card','removeCard:'+card.id,'trash','class="ghost danger"');
  else html='<small>'+story.lore_board.cards.length+' cards · '+story.lore_board.connections.length+' connections</small>';
  let container=$('#inspector');
  if(mobile()){
    if(!openMobile||(!card&&!edge))return;
    modal(edge?'Connection':target.name,'<div class="inspector" style="margin:0;border:0;padding:0">'+html+'</div>');
    container=$('.modal-body');
  }else{container.className='inspector';container.innerHTML=html;}
  const object=edge||(target?.type==='note'?story.lore.find(n=>n.id===card.target):card);
  $$('[data-lore]',container).forEach(input=>input.addEventListener('input',()=>{fieldCheckpoint('lore:'+(edge?.id||card.id)+':'+input.dataset.lore);object[input.dataset.lore]=input.value;changed();}));
  prepareEditors(container);
  $('[data-lore-color]',container)?.addEventListener('input',ev=>{fieldCheckpoint('lore-color:'+(edge?.id||card.id));(edge||card).color=M.rgb(ev.target.value);changed();});
}
function createBoardNote(position){
  checkpoint();const c=M.addNote(story,position||board?.center()||[0,0]);boardView.selected=c.id;boardView.edge=null;changed();board?.focus(c.id);inspectLore();
  const input=$('[data-lore="title"]',overlay)||$('[data-lore="title"]');focusForTyping(input,{select:true});
}
function pinPicker(position){
  const list=[...M.targets(story).values()].filter(t=>t.type!=='note');
  picker('Pin to board',list.map(t=>({id:t.id,name:t.name,subtitle:t.label,portrait:t.portrait})),id=>{checkpoint();const c=M.pin(story,id,position||board?.center()||[0,0]);boardView.selected=c.id;boardView.edge=null;changed();board?.focus(c.id);inspectLore(false);});
}
function removeCard(id){
  const c=story.lore_board.cards.find(c=>c.id===id);if(!c)return;const t=M.targets(story).get(c.target);
  confirmAction(t?.type==='note'?'Delete note?':'Unpin card?',t?.type==='note'?'The note and its connections will be removed. Undo brings them back.':'The original sheet stays in your world.',()=>{
    checkpoint();if(t?.type==='note')M.removeEntity(story,'note',c.target);else M.unpin(story,id);boardView.selected=null;boardView.edge=null;changed();inspectLore(false);
  });
}
function modal(title,html,footer='',cleanup=null){
  closeModal();modalCleanup=cleanup;overlay.innerHTML='<div class="modal-backdrop"><section class="modal" role="dialog" aria-modal="true" aria-label="'+esc(title)+'" tabindex="-1"><header class="modal-header"><h2>'+esc(title)+'</h2>'+ib('close','closeModal','Close')+'</header><div class="modal-body">'+html+'</div>'+(footer?'<footer class="modal-footer">'+footer+'</footer>':'')+'</section></div>';
  $('.modal',overlay).focus();
  $('.modal-backdrop',overlay).addEventListener('click',e=>{if(e.target===e.currentTarget)closeModal();});
}
function closeModal(){modalCleanup?.();modalCleanup=null;overlay.innerHTML='';editKey=null;}
function closeMenu(){menuAbort?.abort();menuElement?.remove();menuElement=null;menuAbort=null;}
function menu(items,anchor){
  closeMenu();const r=anchor instanceof Element?anchor.getBoundingClientRect():null,x=r?r.left:anchor?.x??20,y=r?r.bottom+6:anchor?.y??60;
  menuElement=document.createElement('div');menuElement.className='menu';menuElement.role='menu';
  for(const item of items){if(!item){menuElement.append(document.createElement('hr'));continue;}const b=document.createElement('button');b.type='button';b.setAttribute('role','menuitem');b.innerHTML=(item.icon?icon(item.icon):'')+'<span>'+esc(item.label)+'</span>';if(item.danger)b.className='danger';b.disabled=!!item.disabled;b.addEventListener('click',()=>{closeMenu();Promise.resolve().then(item.run).catch(e=>toast(e.message));});menuElement.append(b);}
  document.body.append(menuElement);const box=menuElement.getBoundingClientRect();menuElement.style.left=Math.max(8,Math.min(x,innerWidth-box.width-8))+'px';menuElement.style.top=Math.max(8,Math.min(y,innerHeight-box.height-8))+'px';
  menuAbort=new AbortController();setTimeout(()=>document.addEventListener('pointerdown',e=>{if(menuElement&&!menuElement.contains(e.target))closeMenu();},{capture:true,signal:menuAbort?.signal}),0);$('button',menuElement)?.focus();
}
function confirmAction(title,text,run){modal(title,'<p class="plain-text">'+esc(text)+'</p>',button('Cancel','closeModal')+button('Continue','confirm','','class="primary"'));$('[data-action="confirm"]',overlay).addEventListener('click',()=>{closeModal();try{run();}catch(e){toast(e.message);}});}
function nameDialog(title,initial,run,submitLabel='Create'){
  modal(title,'<input id="entry-name" aria-label="Name" maxlength="1024" value="'+esc(initial)+'" placeholder="Name">',button('Cancel','closeModal')+button(submitLabel,'confirmName','','class="primary"'));
  const input=$('#entry-name');focusForTyping(input,{select:true});
  const submit=()=>{if(!input.value.trim())return;const name=input.value.trim();closeModal();run(name);};
  $('[data-action="confirmName"]').addEventListener('click',submit);input.addEventListener('keydown',e=>{if(e.key==='Enter')submit();});
}
function picker(title,items,run){
  modal(title,'<input id="picker-search" type="search" placeholder="Search" aria-label="Search entries"><div id="picker-results" class="member-grid"></div>');$('.modal').classList.add('wide');
  function paint(){const q=$('#picker-search').value.toLowerCase();const list=items.filter(i=>i.name.toLowerCase().includes(q)).slice(0,120);$('#picker-results').innerHTML=list.length?list.map(i=>'<button data-pick="'+i.id+'" class="check-row" style="justify-content:flex-start;text-align:left">'+(i.portrait?'<img src="'+imageURL(i.portrait)+'" alt="" width="42" height="42" style="object-fit:cover;border-radius:5px" loading="lazy">':'')+'<span>'+esc(i.name)+(i.subtitle?'<small style="display:block">'+esc(i.subtitle)+'</small>':'')+'</span></button>').join(''):'<p class="muted">No entries</p>';$$('[data-pick]').forEach(b=>b.addEventListener('click',()=>{const id=Number(b.dataset.pick);closeModal();run(id);}));}
  $('#picker-search').addEventListener('input',paint);paint();
}
async function copy(text){try{await navigator.clipboard.writeText(text);toast('Copied');}catch{modal('Copy text','<textarea id="copy-fallback" readonly rows="12">'+esc(text)+'</textarea>');$('#copy-fallback').focus();$('#copy-fallback').select();}}
function styleDialog(){
  modal('Story style','<div class="field"><label>Visual style</label><textarea rows="7" maxlength="250000" data-bind="visual_style" placeholder="The visual identity of this world…">'+esc(story.visual_style)+'</textarea></div><div class="field"><label>Synopsis</label><textarea rows="3" maxlength="250000" data-bind="synopsis">'+esc(story.synopsis)+'</textarea></div>',button('Copy style','copyStyle','copy')+button('Done','closeModal','','class="primary"'));
  bindFields($('.modal-body'),story);
}
function affiliations(){
  const e=entry(),key=section==='characters'?'character_ids':'location_ids';
  modal('Affiliations',story.groups.length?story.groups.map(g=>'<label class="check-row"><input type="checkbox" data-affiliation="'+g.id+'" '+(g[key].includes(e.id)?'checked':'')+'><span>'+esc(g.name)+'</span></label>').join(''):'<p class="muted">Create a group to link it to this sheet.</p>');
  $$('[data-affiliation]').forEach(input=>input.addEventListener('change',()=>{checkpoint();const g=story.groups.find(g=>g.id===Number(input.dataset.affiliation));g[key]=g[key].filter(id=>id!==e.id);if(input.checked)g[key].push(e.id);changed();}));
}
function bindDrop(element,run){
  element?.addEventListener('dragover',e=>{if(e.dataTransfer.types.includes('Files')){e.preventDefault();element.classList.add('dragover');}});
  element?.addEventListener('dragleave',()=>element.classList.remove('dragover'));
  element?.addEventListener('drop',e=>{if(!e.dataTransfer.files.length)return;e.preventDefault();element.classList.remove('dragover');run([...e.dataTransfer.files]).catch(err=>toast(err.message));});
}
function chooseImages(target){imageHandler=files=>uploadImages(files,target);$('#image-file').value='';$('#image-file').click();}
async function uploadImages(files,target){
  if(!files.length)return;
  if(files.some(f=>f.name.toLowerCase().endsWith('.scro'))){await importWorld(files.find(f=>f.name.toLowerCase().endsWith('.scro')));return;}
  await flush();checkpoint();
  await busy('Importing original images…',async progress=>{
    let i=0;
    for(const file of files){
      if(!['image/png','image/jpeg','image/webp'].includes(file.type))throw Error('Choose PNG, JPEG, or WebP images.');
      const e=target?(target.type==='characters'?asset(target.id):story[target.type].find(e=>e.id===target.id)):null;
      if(target?.type==='characters'&&e.character.references.length>=24)throw Error('A character can have up to 24 additional references.');
      if(target?.type==='groups'&&e.reference_ids.length>=64)throw Error('A group can have up to 64 references.');
      const form=new FormData();form.append('file',file);form.append('revision',revision);form.append('kind',target?.type==='locations'||(!target&&imageFilter==='Backgrounds')?'Background':'Image');
      const result=await api('upload',{method:'POST',form});story.assets.push(result.asset);revision=result.revision;story.next_id=Math.max(story.next_id,result.next_id);
      if(target)M.attachReference(story,target.type,e,result.asset.id);
      serial++;await flush();progress(Math.round(++i/files.length*100));
    }
  });renderWorkspace();status();
}
function referenceMenu(id,anchor){
  const e=entry(),type=section,a=asset(id),refs=M.references(story,type,e),index=refs.findIndex(r=>r.id===id);
  const remove=()=>{checkpoint();if(type==='characters')e.character.references=e.character.references.filter(r=>r.asset_id!==id);else{const key=type==='locations'?'asset_ids':'reference_ids';e[key]=e[key].filter(n=>n!==id);}changed(true);};
  const move=direction=>{checkpoint();const list=type==='characters'?e.character.references:type==='locations'?e.asset_ids:e.reference_ids;const i=list.findIndex(v=>typeof v==='object'?v.asset_id===id:v===id),j=i+direction;if(i>=0&&j>=0&&j<list.length)[list[i],list[j]]=[list[j],list[i]];changed(true);};
  menu([{label:M.thumbnail(story,e.id,null)===id?'Card thumbnail':'Use as thumbnail',icon:'images',disabled:M.thumbnail(story,e.id,null)===id,run:()=>{checkpoint();M.setThumbnail(story,e.id,id);changed();toast('Card thumbnail updated');}},{label:'View original',icon:'images',run:()=>viewer(id)},{label:'Details',icon:'note',run:()=>referenceDetails(id)},{label:'Download original',icon:'export',run:()=>downloadOriginal(id)},null,{label:'Move earlier',run:()=>move(-1),disabled:index===0},{label:'Move later',run:()=>move(1),disabled:index===refs.length-1},{label:'Remove reference',icon:'trash',danger:true,disabled:type==='characters'&&id===e.id,run:remove}],anchor);
}
function referenceDetails(id){
  const e=entry(),reference=section==='characters'?e?.character.references.find(r=>r.asset_id===id):null,a=asset(id),object=reference||a,name=reference?'label':'name',notes=reference?'notes':'notes';
  modal('Reference details','<div class="field"><label>Name</label><input data-bind="'+name+'" value="'+esc(object[name])+'" maxlength="1024"></div><div class="field"><label>Notes</label><textarea data-bind="'+notes+'" rows="6" maxlength="250000">'+esc(object[notes])+'</textarea></div><small>'+a.width+' × '+a.height+' px · original preserved</small>');
  bindFields($('.modal-body'),object);
}
function imageDetails(id){
  const a=asset(id);if(!a)return;
  modal('Image details','<div class="field"><label>Name</label><input data-bind="name" aria-label="Image name" maxlength="1024" value="'+esc(a.name)+'"></div><div class="field"><label>Notes</label><textarea data-bind="notes" rows="4" maxlength="250000">'+esc(a.notes)+'</textarea></div><div class="field"><label>Prompt</label><textarea data-bind="prompt" rows="3" maxlength="250000">'+esc(a.prompt)+'</textarea></div>'+(a.kind==='Background'?'<div class="field"><label>Location</label><select id="image-location"><option value="">Unfiled</option>'+story.locations.map(l=>'<option value="'+l.id+'" '+(l.asset_ids.includes(id)?'selected':'')+'>'+esc(l.name)+'</option>').join('')+'</select></div>':'')+'<small>'+a.width+' × '+a.height+' px</small>');
  bindFields($('.modal-body'),a);
  $('#image-location')?.addEventListener('change',event=>{checkpoint();for(const l of story.locations)l.asset_ids=l.asset_ids.filter(n=>n!==id);const l=story.locations.find(l=>l.id===Number(event.target.value));if(l)l.asset_ids.push(id);changed();});
}
function downloadOriginal(id){const a=document.createElement('a');a.href=imageURL(id,'original',true);a.download='reference';document.body.append(a);a.click();a.remove();}
function viewer(id){
  const a=asset(id);if(!a)return;
  modal(a.name,'<div class="viewer" id="image-viewer"><img id="view-image" src="'+imageURL(id,'1280')+'" alt="'+esc(a.name)+'" draggable="false"></div><div class="viewer-toolbar"><small>'+a.width+' × '+a.height+' px</small>'+button('1:1','actualSize')+button('Fit','fitImage')+button('Original','downloadOriginal:'+id,'export')+button('Copy','copyImage:'+id,'copy')+button('Details','imageDetails:'+id,'note')+'</div>');
  $('.modal').classList.add('viewer-modal');
  const img=$('#view-image'),view=$('#image-viewer');let zoom=1,pan={x:0,y:0},original=false,points=new Map(),gesture=null;
  const update=()=>{img.style.transform='translate('+pan.x+'px,'+pan.y+'px) scale('+zoom+')';if(zoom>1.15&&!original){original=true;img.src=imageURL(id,'original');}};
  $('[data-action="actualSize"]').onclick=()=>{const box=img.getBoundingClientRect();const displayed=box.width/zoom;zoom=Math.min(12,a.width/Math.max(1,displayed));pan={x:0,y:0};original=true;img.src=imageURL(id,'original');update();};
  $('[data-action="fitImage"]').onclick=()=>{zoom=1;pan={x:0,y:0};update();};
  view.addEventListener('wheel',e=>{e.preventDefault();zoom=Math.max(1,Math.min(12,zoom*Math.exp(-e.deltaY*.002)));update();},{passive:false});
  view.addEventListener('pointerdown',e=>{view.setPointerCapture(e.pointerId);points.set(e.pointerId,{x:e.clientX,y:e.clientY});gesture=points.size===2?{distance:distance([...points.values()]),zoom}:null;});
  view.addEventListener('pointermove',e=>{if(!points.has(e.pointerId))return;const old=points.get(e.pointerId);points.set(e.pointerId,{x:e.clientX,y:e.clientY});if(points.size===2&&gesture){zoom=Math.max(1,Math.min(12,gesture.zoom*distance([...points.values()])/Math.max(1,gesture.distance)));}else if(zoom>1){pan.x+=e.clientX-old.x;pan.y+=e.clientY-old.y;}update();});
  const end=e=>{points.delete(e.pointerId);gesture=null;};view.addEventListener('pointerup',end);view.addEventListener('pointercancel',end);
}
function distance(points){return Math.hypot(points[0].x-points[1].x,points[0].y-points[1].y);}
async function copyImage(id){
  if(!navigator.clipboard?.write||!window.ClipboardItem){toast('Use Original to download this image.');return;}
  try{const promise=fetch(imageURL(id,'original')).then(r=>r.blob()).then(async blob=>{if(blob.type==='image/png')return blob;const bitmap=await createImageBitmap(blob);const canvas=document.createElement('canvas');canvas.width=bitmap.width;canvas.height=bitmap.height;canvas.getContext('2d').drawImage(bitmap,0,0);bitmap.close();return new Promise(resolve=>canvas.toBlob(resolve,'image/png'));});await navigator.clipboard.write([new ClipboardItem({'image/png':promise})]);toast('Original image copied');}catch{toast('Clipboard unavailable here. Use Original to download.');}
}
async function exportSheet(){
  const e=entry(),refs=M.references(story,section,e);if(!refs.length){toast('Add a reference first.');return;}
  await busy('Creating reference sheet…',async()=>{
    const width=1800,columns=Math.min(3,refs.length),rows=Math.ceil(refs.length/columns),cell=width/columns,height=Math.min(4400,100+rows*cell);
    const canvas=document.createElement('canvas');canvas.width=width;canvas.height=height;const ctx=canvas.getContext('2d');ctx.fillStyle='#fff';ctx.fillRect(0,0,width,height);ctx.fillStyle='#202124';ctx.font='36px sans-serif';ctx.fillText(e.name,30,60);
    const ch=(height-100)/rows;
    for(let i=0;i<refs.length;i++){
      const blob=await fetch(imageURL(refs[i].id,'original')).then(r=>r.blob()),image=await createImageBitmap(blob);
      const scale=Math.min((cell-30)/image.width,(ch-40)/image.height),w=image.width*scale,h=image.height*scale,x=(i%columns)*cell+(cell-w)/2,y=100+Math.floor(i/columns)*ch+(ch-40-h)/2;
      ctx.drawImage(image,x,y,w,h);image.close();ctx.font='18px sans-serif';ctx.fillText((refs[i].label||refs[i].name).slice(0,50),(i%columns)*cell+16,100+(Math.floor(i/columns)+1)*ch-12);
    }
    const blob=await new Promise(resolve=>canvas.toBlob(resolve,'image/png')),url=URL.createObjectURL(blob),a=document.createElement('a');a.href=url;a.download=e.name+' - references.png';a.click();setTimeout(()=>URL.revokeObjectURL(url),30000);
  });
}
function entityMenu(anchor){
  const e=entry(),type=section;
  menu([{label:'Rename',icon:'edit',run:()=>nameDialog('Rename',e.name,name=>{checkpoint();e.name=name;changed(true);},'Save')},{label:'Pin to lore board',icon:'lore',run:()=>{checkpoint();const c=M.pin(story,e.id,[0,0]);changed();section='lore';selected=null;boardView.selected=c.id;render();board.focus(c.id);}},{label:'Copy prompt',icon:'copy',run:()=>copy(currentPrompt())},{label:'Export reference sheet',icon:'export',run:exportSheet},null,{label:'Delete '+(type==='characters'?'character':type==='locations'?'location':'group'),icon:'trash',danger:true,run:()=>confirmAction('Delete '+e.name+'?','Reference images stay in the library. This can be undone.',()=>{checkpoint();M.removeEntity(story,type,e.id);selected=null;changed(true);})}],anchor);
}
function mobileMenu(anchor){menu([...sections.filter(([k])=>!['characters','locations','lore'].includes(k)).map(([key,label])=>({label,icon:key,run:()=>navigate(key)})),null,{label:'Story style',icon:'style',run:styleDialog},{label:'Undo',icon:'undo',run:()=>travel('undo'),disabled:!history.undo.length},{label:'Redo',icon:'redo',run:()=>travel('redo'),disabled:!history.redo.length}],anchor);}
function fileMenu(anchor){menu([{label:'Open .scro…',icon:'open',run:()=>$('#open-file').click()},{label:'New world',icon:'plus',run:newWorld},...(story?[{label:'Rename world',icon:'note',run:()=>nameDialog('World name',story.title,name=>{checkpoint();story.title=name;changed();},'Save')},{label:'Export .scro',icon:'export',run:exportWorld},{label:'Save status',icon:'check',run:saveStatus},null,{label:'Start screen',run:async()=>{await flush();const session=await api('session',{id:null});recent=session.recent;story=null;project=null;render();}}]:[]),null,{label:'Download desktop app',run:()=>{location.href='/#download';}}],anchor);}
function saveStatus(){
  modal('Save status','<p class="plain-text">'+esc(saveError?.message||(serial===savedSerial?'Your edits are saved in this temporary workspace. Export a .scro file to keep them.':'Saving your recent edits…'))+'</p>',(saveError?.status===409?button('Save a separate copy','forkWorld','','class="primary"'):button('Retry','retrySave')+button('Export .scro','export','export','class="primary"')));
}
function travel(direction){if(!story)return;const next=history.travel(story,direction);if(!next)return;const counter=story.next_id;story=next;story.next_id=Math.max(counter,story.next_id);editKey=null;changed(true);status();}
async function action(name,element){
  const [key,value]=name.split(':');const n=Number(value);
  if(key==='imageDetails')return imageDetails(n);
  if(key==='file')return fileMenu(element);
  if(key==='open')return $('#open-file').click();
  if(key==='newWorld')return newWorld();
  if(key==='recent')return openWorld(value);
  if(key==='nav')return navigate(value);
  if(key==='mobileMenu')return mobileMenu(element);
  if(key==='newMenu'&&!story)return newWorld();
  if(key==='newMenu')return menu([['characters','Character'],['locations','Location'],['groups','Group']].map(([type,label])=>({label,icon:type,run:()=>{navigate(type);action('addEntity');}})),element);
  if(key==='export')return exportWorld();
  if(key==='style')return styleDialog();
  if(key==='copyStyle')return copy(story.visual_style||'');
  if(key==='saveStatus')return saveStatus();
  if(key==='forkWorld'){clearTimeout(saveTimer);await busy('Preserving your copy…',async()=>{if(saving)await saving.catch(()=>{});const result=await api('fork',{method:'POST',data:{story,revision}});closeModal();adopt(result);});return toast('Your separate copy is ready to export');}
  if(key==='retrySave'){await flush();closeModal();return toast('Saved temporarily');}
  if(key==='closeModal')return closeModal();
  if(key==='undo'||key==='redo')return travel(key);
  if(key==='addEntity')return nameDialog('New '+(section==='characters'?'character':section==='locations'?'location':'group'),'',name=>{checkpoint();const e=M.addEntity(story,section,name);selected=e.id;sheetTab='References';changed(true);});
  if(key==='select'){galleryPosition=$('#gallery-content')?.scrollTop||0;selected=n;sheetTab='References';subfield=null;return renderWorkspace();}
  if(key==='backGallery'){selected=null;return renderWorkspace();}
  if(key==='sheet'){sheetTab=value;subfield=null;return renderWorkspace();}
  if(key==='subfield'){subfield=value;return renderWorkspace();}
  if(key==='loadMore'){galleryLimit+=60;return renderWorkspace();}
  if(key==='filter'){imageFilter=value;return renderWorkspace();}
  if(key==='gender'){checkpoint();entry().character.gender=entry().character.gender===value?null:value;return changed(true);}
  if(key==='affiliations')return affiliations();
  if(key==='entityMenu')return entityMenu(element);
  if(key==='copyPrompt')return copy(currentPrompt());
  if(key==='angle')return showAngle({modal,closeModal,copy,prompt:currentPrompt,character:section==='characters'});
  if(key==='addReference')return chooseImages({type:section,id:selected});
  if(key==='importLibrary')return chooseImages(null);
  if(key==='pickReference'){
    const e=entry(),type=section;return picker('Reference library',story.assets.filter(a=>a.width>0&&(type==='groups'||a.kind===(type==='locations'?'Background':'Image'))).map(a=>({id:a.id,name:a.name,portrait:a.id})),id=>{checkpoint();M.attachReference(story,type,e,id);changed(true);});
  }
  if(key==='view')return viewer(n);
  if(key==='refMenu')return referenceMenu(n,element);
  if(key==='downloadOriginal')return downloadOriginal(n);
  if(key==='copyImage')return copyImage(n);
  if(key==='exportSheet')return exportSheet();
  if(key==='addOutfit'){if(entry().character.outfits.length>=32)return toast('Up to 32 outfits per character.');checkpoint();entry().character.outfits.push({name:'New outfit',description:''});subfield=String(entry().character.outfits.length-1);return changed(true);}
  if(key==='activeOutfit'){checkpoint();entry().character.active_outfit=n;return changed(true);}
  if(key==='removeOutfit'){checkpoint();const p=entry().character;p.outfits.splice(n,1);p.active_outfit=p.active_outfit===n?null:p.active_outfit>n?p.active_outfit-1:p.active_outfit;subfield=null;return changed(true);}
  if(key==='addColor'){if(entry().character.palette.length>=16)return toast('Up to 16 colors per character.');checkpoint();entry().character.palette.push({name:'',color:[114,168,213,255]});return changed(true);}
  if(key==='removeColor'){checkpoint();entry().character.palette.splice(n,1);return changed(true);}
  if(key==='addNote')return createBoardNote();
  if(key==='pinCard')return pinPicker();
  if(key==='connectMode'){closeModal();board?.startLink(boardView.selected);return;}
  if(key==='fitBoard')return board?.fit();
  if(key==='openTarget'){closeModal();return openTarget(n);}
  if(key==='removeCard')return removeCard(n);
  if(key==='removeEdge'){checkpoint();story.lore_board.connections=story.lore_board.connections.filter(e=>e.id!==boardView.edge);boardView.edge=null;closeModal();changed();return inspectLore(false);}
  if(key==='timeUp'||key==='timeDown'){const list=M.timeline(story),index=list.findIndex(e=>e.chapter_id===n);if((key==='timeUp'&&index===0)||(key==='timeDown'&&index===list.length-1))return;checkpoint();M.reorderTimeline(story,n,key==='timeUp'?list[index-1].chapter_id:list[index+2]?.chapter_id??null);return changed(true);}
  if(key==='clearDraft'){const db=await database();db.transaction('draft','readwrite').objectStore('draft').clear();return toast('Browser recovery draft cleared');}
}
document.addEventListener('click',e=>{const b=e.target.closest('[data-action]');if(!b)return;Promise.resolve().then(()=>action(b.dataset.action,b)).catch(error=>toast(error.message));});
$('#open-file').addEventListener('change',e=>{importWorld(e.target.files[0]).catch(err=>toast(err.message));e.target.value='';});
$('#image-file').addEventListener('change',e=>{imageHandler?.([...e.target.files]).catch(err=>toast(err.message));e.target.value='';});
document.addEventListener('keydown',e=>{
  if(e.key==='Escape'){closeMenu();closeModal();board?.cancelLink();return;}
  if(overlay.firstChild && e.key==='Tab'){
    const focusable=$$('button:not(:disabled),input,textarea,select,a[href],[tabindex="0"]',overlay).filter(e=>e.getClientRects().length);const first=focusable[0],last=focusable.at(-1);if(e.shiftKey&&document.activeElement===first){e.preventDefault();last?.focus();}else if(!e.shiftKey&&document.activeElement===last){e.preventDefault();first?.focus();}return;
  }
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='s'){e.preventDefault();exportWorld().catch(err=>toast(err.message));return;}
  if(e.target.matches('input,textarea,select,[contenteditable]'))return;
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='z'){e.preventDefault();travel(e.shiftKey?'redo':'undo');}
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='y'){e.preventDefault();travel('redo');}
});
document.addEventListener('paste',e=>{if(!story||e.target.matches('input,textarea'))return;const files=[...e.clipboardData.files].filter(f=>f.type.startsWith('image/'));if(files.length){e.preventDefault();uploadImages(files,selected&&entry()?{type:section,id:selected}:null).catch(err=>toast(err.message));}});
window.addEventListener('beforeunload',e=>{if(story && (serial!==savedSerial||saveError)){e.preventDefault();e.returnValue='';}});
document.addEventListener('visibilitychange',()=>{if(document.hidden){draftWrite();flush().catch(()=>{});}});
window.addEventListener('online',()=>flush().catch(reportSaveError));
matchMedia('(max-width:680px)').addEventListener('change',()=>{if(story){closeModal();render();}});
async function boot(){
  try{
    const session=await api('session',{id:null});csrf=session.csrf;recent=session.recent;
    const draft=await draftRead();
    if(draft && recent.some(r=>r.id===draft.project)){
      const data=await api('load',{id:draft.project});adopt(data,false);
      if(draft.dirty){story=draft.story;serial++;render();if(draft.revision===data.revision){changed();toast('Recovered your recent edits');}else{saveError=Object.assign(Error('Another tab changed this world. Save your recovered edits as a separate copy.'),{status:409});status();saveStatus();}}else draftWrite();
    }else render();
  }catch(error){app.innerHTML='<header class="topbar"><a href="/">← Scroll Studio</a></header><main class="welcome"><div class="empty-inner"><h1>Could not open the editor</h1><p class="plain-text">'+esc(error.message)+'</p>'+button('Try again','reload','','class="primary"')+'</div></main>';$('[data-action="reload"]').onclick=()=>location.reload();}
}
boot();

