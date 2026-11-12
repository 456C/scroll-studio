// World-only operations. Original chapter data and image bytes never enter undo snapshots twice.
export const clone = value => structuredClone(value);
export function profile() {
  return {...Object.fromEntries(['tagline','role','age','pronouns','personality','backstory','relationships','face','hair','eyes','skin','build','distinguishing_features','consistency','avoid'].map(k=>[k,''])),gender:null,outfits:[],active_outfit:null,palette:[],references:[]};
}
export function id(s) { if(!Number.isSafeInteger(s.next_id)||s.next_id>=Number.MAX_SAFE_INTEGER)throw Error('This world needs desktop editing.');return s.next_id++; }
export function addEntity(s,type,name) {
  if((type==='characters'?s.assets:s[type]).length>=4096)throw Error('This world has reached its entry limit.');
  const n=id(s);let entity;
  if(type==='characters'){entity={id:n,name,kind:'Character',width:0,height:0,notes:'',prompt:'',character:profile()};s.assets.push(entity);}
  if(type==='locations'){entity={id:n,name,asset_ids:[],profile:Object.fromEntries(['description','atmosphere','lighting','consistency','notes','prompt','avoid'].map(k=>[k,'']))};s.locations.push(entity);}
  if(type==='groups'){entity={id:n,name,description:'',color:[114,190,182],reference_ids:[],character_ids:[],location_ids:[]};s.groups.push(entity);}
  return entity;
}
export function thumbnail(s,target,fallback) {return s.card_thumbnails?.find(c=>c.target===target)?.asset_id??fallback;}
export function setThumbnail(s,target,assetId) {s.card_thumbnails??=[];s.card_thumbnails=s.card_thumbnails.filter(c=>c.target!==target);s.card_thumbnails.push({target,asset_id:assetId});}
export function pruneThumbnails(s){
 const allowed=new Map(s.assets.filter(a=>a.kind==='Character').map(a=>[a.id,new Set([...(a.width>0?[a.id]:[]),...a.character.references.map(r=>r.asset_id)])]));
 for(const l of s.locations)allowed.set(l.id,new Set(l.asset_ids));for(const g of s.groups)allowed.set(g.id,new Set(g.reference_ids));
 s.card_thumbnails=(s.card_thumbnails||[]).filter(c=>allowed.get(c.target)?.has(c.asset_id));
}
export function targets(s) {
  return new Map([...s.assets.filter(a=>a.kind==='Character').map(a=>[a.id,{id:a.id,type:'characters',label:'Character',name:a.name,portrait:thumbnail(s,a.id,a.width>0?a.id:a.character.references[0]?.asset_id)}]),
    ...s.locations.map(l=>[l.id,{id:l.id,type:'locations',label:'Location',name:l.name,portrait:thumbnail(s,l.id,l.asset_ids[0])}]),
    ...s.groups.map(g=>[g.id,{id:g.id,type:'groups',label:'Group',name:g.name,portrait:thumbnail(s,g.id,g.reference_ids[0])}]),
    ...s.lore.map(n=>[n.id,{id:n.id,type:'note',label:'Note',name:n.title,text:n.text}])]);
}
export function references(s,type,entry) {
  const assetMap=new Map(s.assets.map(a=>[a.id,a]));
  if(type==='characters') {
    const refs=entry.character.references.map(r=>({...assetMap.get(r.asset_id),label:r.label,referenceNote:r.notes,ref:r})).filter(a=>a.id);
    return entry.width>0?[entry,...refs]:refs;
  }
  return (type==='locations'?entry.asset_ids:entry.reference_ids).map(id=>assetMap.get(id)).filter(Boolean);
}
export function pin(s,target,position=[0,0]) {
  const existing=s.lore_board.cards.find(c=>c.target===target);if(existing)return existing;
  if(s.lore_board.cards.length>=8192)throw Error('The board has reached its card limit.');
  const t=targets(s).get(target);if(!t)throw Error('This entry no longer exists.');
  if(s.lore_board.cards.some(c=>Math.abs(c.position[0]-position[0])<230 && Math.abs(c.position[1]-position[1])<255)){
    const origin=[...position];let found=false;
    for(let ring=1;ring<100&&!found;ring++)for(let step=0;step<ring*8&&!found;step++){const angle=step/(ring*8)*Math.PI*2,candidate=[origin[0]+Math.round(Math.cos(angle)*ring)*260,origin[1]+Math.round(Math.sin(angle)*ring)*280];if(!s.lore_board.cards.some(c=>Math.abs(c.position[0]-candidate[0])<230&&Math.abs(c.position[1]-candidate[1])<255)){position=candidate;found=true;}}
  }
  const c={id:id(s),target,position,note:'',color:t.type==='note'?[222,181,105]:[114,168,213]};s.lore_board.cards.push(c);return c;
}
export function addNote(s,position=[0,0]) {
  if(s.lore.length>=4096)throw Error('The world has reached its note limit.');
  const note={id:id(s),title:'Untitled note',text:'',links:[]};s.lore.push(note);return pin(s,note.id,position);
}
export function connect(s,from,to) {
  if(from===to)return null;
  const existing=s.lore_board.connections.find(e=>(e.from===from&&e.to===to)||(e.from===to&&e.to===from));if(existing)return existing;
  if(s.lore_board.connections.length>=16384)throw Error('The board has reached its connection limit.');
  if(![from,to].every(id=>s.lore_board.cards.some(c=>c.id===id)))return null;
  const edge={id:id(s),from,to,label:'',color:[190,103,106]};s.lore_board.connections.push(edge);return edge;
}
export function unpin(s,cardId) {
  s.lore_board.cards=s.lore_board.cards.filter(c=>c.id!==cardId);
  s.lore_board.connections=s.lore_board.connections.filter(e=>e.from!==cardId&&e.to!==cardId);
}
export function removeEntity(s,type,entityId) {
  if(type==='characters' && s.chapters.some(c=>c.layers.some(l=>l.kind?.Image?.asset_id===entityId)))throw Error('This character image is used in a chapter. Keep it in the world.');
  if(type==='characters')s.assets=s.assets.filter(a=>a.id!==entityId);
  if(type==='locations')s.locations=s.locations.filter(a=>a.id!==entityId);
  if(type==='groups')s.groups=s.groups.filter(a=>a.id!==entityId);
  if(type==='note')s.lore=s.lore.filter(a=>a.id!==entityId);
  for(const g of s.groups){g.character_ids=g.character_ids.filter(id=>id!==entityId);g.location_ids=g.location_ids.filter(id=>id!==entityId);}
  for(const n of s.lore)n.links=n.links.filter(id=>id!==entityId);
  for(const entry of s.timeline||[]) if(entry.event) entry.event.links=entry.event.links.filter(id=>id!==entityId);
  for(const c of s.lore_board.cards.filter(c=>c.target===entityId))unpin(s,c.id);
}
export function entryId(e){return e.event?.id||e.chapter_id;}
export function isEvent(e){return !!e.event;}
export const eventKinds=['Flashback','Lore','Other'];
export function linkChoices(s){
  return [...s.assets.filter(a=>a.kind==='Character').map(a=>({id:a.id,name:a.name,kind:'Character'})),
    ...s.locations.map(l=>({id:l.id,name:l.name,kind:'Location'})),
    ...s.groups.map(g=>({id:g.id,name:g.name,kind:'Group'}))];
}
export function attachReference(s,type,entry,assetId) {
  const a=s.assets.find(a=>a.id===assetId);if(!a)return;
  if(type==='characters') {
    if(a.kind!=='Image')throw Error('Character references use images from the reference library.');
    if(entry.character.references.some(r=>r.asset_id===assetId))return;
    if(entry.character.references.length>=24)throw Error('A character can have up to 24 additional references.');
    entry.character.references.push({asset_id:assetId,label:a.name,notes:''});
  } else if(type==='locations') {
    if(a.kind!=='Background')throw Error('Choose a background image.');
    for(const l of s.locations)l.asset_ids=l.asset_ids.filter(n=>n!==assetId);
    entry.asset_ids.push(assetId);
  } else {
    if(!a.width)throw Error('Choose an image reference.');
    if(!entry.reference_ids.includes(assetId)){if(entry.reference_ids.length>=64)throw Error('A group can have up to 64 references.');entry.reference_ids.push(assetId);}
  }
}
export function timeline(s) {
  const seen=new Set(s.timeline.filter(t=>!t.event).map(t=>t.chapter_id));
  return [...s.timeline,...s.chapters.filter(c=>!seen.has(c.id)).map(c=>({chapter_id:c.id,when:'',summary:''}))];
}
export function reorderTimeline(s,id,beforeId=null) {
  const all=timeline(s),entry=all.find(e=>entryId(e)===id);if(!entry||id===beforeId)return;
  const rest=all.filter(e=>e!==entry),index=rest.findIndex(e=>entryId(e)===beforeId);rest.splice(index<0?rest.length:index,0,entry);s.timeline=rest;
}
export function insertEvent(s,afterId=null) {
  if(timeline(s).filter(t=>t.event).length>=4096)throw Error('This timeline has reached its event limit.');
  const eventId=id(s);
  const entry={chapter_id:0,when:'',summary:'',event:{id:eventId,title:'',kind:'Other',links:[]}};
  const all=timeline(s),index=afterId==null?all.length:all.findIndex(e=>entryId(e)===afterId);
  if(afterId!=null&&index<0)throw Error('Timeline item no longer exists.');
  all.splice(index<0?all.length:index+1,0,entry);s.timeline=all;return eventId;
}
export function deleteEvent(s,eventId) {
  if(!timeline(s).some(e=>e.event?.id===eventId))throw Error('Event no longer exists.');
  s.timeline=timeline(s).filter(e=>e.event?.id!==eventId);
}
export function hex(color) {return '#'+color.slice(0,3).map(v=>Math.max(0,Math.min(255,v)).toString(16).padStart(2,'0')).join('');}
export function rgb(hex) {return [1,3,5].map(i=>parseInt(hex.slice(i,i+2),16));}
export function prompt(s,type,e,kind='Portrait',scene='') {
  const chunks=[],part=(title,value)=>{if(value?.trim())chunks.push(title+'\n'+value.trim());};
  part('STORY VISUAL STYLE',s.visual_style);
  if(type==='characters') {
    const p=e.character;
    part('CHARACTER',[['Name',e.name],['Concept',p.tagline],['Role',p.role],['Age',p.age],['Gender',p.gender],['Personality / body language',p.personality]].filter(([,v])=>v?.trim()).map(([k,v])=>k+': '+v).join('\n'));
    part('APPEARANCE',[['Face',p.face],['Hair',p.hair],['Eyes',p.eyes],['Skin',p.skin],['Build / silhouette',p.build],['Distinguishing features',p.distinguishing_features]].filter(([,v])=>v?.trim()).map(([k,v])=>k+': '+v).join('\n'));
    const outfit=p.outfits[p.active_outfit];if(outfit)part('SELECTED OUTFIT',[outfit.name,outfit.description].filter(Boolean).join(': '));
    part('COLOR PALETTE',p.palette.map(sw=>hex(sw.color)).join(', '));part('CONTINUITY NOTES',e.notes);part('KEEP CONSISTENT',p.consistency);part('ADDITIONAL DIRECTION',e.prompt);part('AVOID',p.avoid);
    const requests={Portrait:'Create a clear chest-up character portrait with a readable face and silhouette, a neutral background, and no lettering.',Turnaround:'Create a character turnaround sheet: full-body front, side, and back views of the SAME character at the same scale. Neutral poses and a plain background. No lettering or speech bubbles.',Expressions:'Create an expression reference sheet of the SAME character: neutral, happy, angry, sad, surprised, and determined. Plain background; no lettering.',Scene:'Create a webtoon illustration of this character in the scene below. Leave dialogue and speech bubbles for Scroll Studio.'};
    part('IMAGE REQUEST',(requests[kind]||requests.Portrait)+' Preserve the specified identity, proportions, palette, and outfit.');
    if(kind==='Scene')part('SCENE / POSE',scene);
    part('REFERENCE NOTES',p.references.filter(r=>r.notes.trim()).map(r=>r.label+': '+r.notes).join('\n'));
  } else if(type==='locations') {
    part('LOCATION',e.name);
    for(const [key,title]of Object.entries({description:'DESCRIPTION',atmosphere:'ATMOSPHERE',lighting:'LIGHTING',consistency:'KEEP CONSISTENT',notes:'CONTINUITY NOTES',prompt:'ADDITIONAL DIRECTION',avoid:'AVOID'}))part(title,e.profile[key]);
    part('CAMERA / SCENE',scene);part('IMAGE REQUEST','Create a webtoon background of this location. Preserve its landmarks, architecture, proportions, materials, lighting and visual identity. No lettering or speech bubbles.');
  } else {part('GROUP',e.name);part('IDENTITY',e.description);}
  const groups=s.groups.filter(g=>type==='characters'?g.character_ids.includes(e.id):type==='locations'?g.location_ids.includes(e.id):false);
  for(const g of groups)part('AFFILIATION: '+g.name,g.description||g.name);
  if(references(s,type,e).length)part('REFERENCE IMAGES','Use the attached original reference images as visual anchors. Keep the same design instead of redesigning it.');
  return chunks.join('\n\n');
}
export class History {
  constructor(budget=16*1024*1024){this.budget=budget;this.undo=[];this.redo=[];}
  push(story){const text=JSON.stringify(story);if(this.undo.at(-1)===text)return;this.undo.push(text);this.redo=[];while(this.undo.length>1 && (this.undo.length>40 || this.undo.reduce((n,s)=>n+s.length*2,0)>this.budget))this.undo.shift();}
  travel(story,direction){const source=this[direction],destination=this[direction==='undo'?'redo':'undo'];if(!source.length)return null;destination.push(JSON.stringify(story));return JSON.parse(source.pop());}
}

