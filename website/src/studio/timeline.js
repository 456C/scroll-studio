import {icon,escape as esc} from './icons.js';

// Returns the first card to the right of the insertion slot. Null means the end.
export function insertionBefore(cards,x) {
  return cards.find(card=>x<card.center)?.id??null;
}
export function wouldMove(ids,id,before) {
  const index=ids.indexOf(id);
  return index>=0 && before!==id && (ids[index+1]??null)!==before;
}

export class TimelineBoard {
  constructor(root,{entries,chapters,view,move,edit}) {
    Object.assign(this,{root,entries,chapters,view,move,edit});this.abort=new AbortController();
    if(!entries.some(e=>e.chapter_id===view.selected))view.selected=entries[0]?.chapter_id??null;
    root.innerHTML='<div class="workspace-header"><h1>Timeline</h1><small>Chronology</small></div><div class="timeline-rail" tabindex="0" aria-label="Chapter chronology"><div class="timeline-track"></div></div><section class="timeline-detail"></section><span class="sr-only" role="status" aria-live="polite"></span>';
    this.rail=root.querySelector('.timeline-rail');this.track=root.querySelector('.timeline-track');this.detail=root.querySelector('.timeline-detail');
    this.track.innerHTML=entries.map((e,i)=>'<article class="chapter-stop" data-chapter="'+e.chapter_id+'"><div class="chapter-node">'+String(i+1).padStart(2,'0')+'</div><div class="chapter-card" role="button" tabindex="0" aria-label="'+esc(chapters.find(c=>c.id===e.chapter_id)?.name||'Chapter')+'"><div class="chapter-symbol">'+icon('timeline')+'</div><h3>'+esc(chapters.find(c=>c.id===e.chapter_id)?.name||'Chapter')+'</h3><small>'+esc(e.when||'Undated')+'</small><p>'+esc(e.summary||'')+'</p><button type="button" class="chapter-grip ghost" aria-label="Drag chapter" title="Drag to reorder · Alt + arrow keys">'+icon('grip')+'</button></div></article>').join('');
    if(!entries.length)this.track.innerHTML='<div class="empty"><h2>No chapters yet</h2></div>';
    this.marker=document.createElement('div');this.marker.className='timeline-insertion';this.marker.hidden=true;this.track.append(this.marker);
    this.rail.scrollLeft=view.scroll||0;this.select(view.selected);
    const listen=(el,type,fn,options={})=>el.addEventListener(type,fn,{...options,signal:this.abort.signal});
    listen(this.rail,'scroll',()=>{view.scroll=this.rail.scrollLeft;},{passive:true});
    listen(this.track,'pointerdown',e=>this.down(e));
    listen(this.track,'pointermove',e=>this.pointerMove(e));
    listen(this.track,'pointerup',e=>this.end(e,false));
    listen(this.track,'pointercancel',e=>this.end(e,true));
    listen(this.track,'lostpointercapture',e=>this.end(e,true));
    listen(this.track,'click',e=>{if(this.suppressClick){e.preventDefault();return;}const card=e.target.closest('[data-chapter]');if(card)this.select(Number(card.dataset.chapter));});
    listen(this.track,'keydown',e=>{
      const card=e.target.closest('[data-chapter]');if(!card)return;const id=Number(card.dataset.chapter),index=entries.findIndex(v=>v.chapter_id===id);
      if(e.altKey&&['ArrowLeft','ArrowRight'].includes(e.key)){e.preventDefault();this.step(id,e.key==='ArrowLeft'?-1:1);}
      else if(e.key==='Enter'||e.key===' '){e.preventDefault();this.select(id);}
      else if(['ArrowLeft','ArrowRight','Home','End'].includes(e.key)){
        e.preventDefault();const at=e.key==='Home'?0:e.key==='End'?entries.length-1:Math.max(0,Math.min(entries.length-1,index+(e.key==='ArrowLeft'?-1:1)));
        const next=this.track.querySelector('[data-chapter="'+entries[at].chapter_id+'"] .chapter-card');next.focus({preventScroll:true});next.scrollIntoView({block:'nearest',inline:'nearest'});this.select(entries[at].chapter_id);
      }
    });
    listen(document,'keydown',e=>{if(e.key==='Escape'&&this.drag){e.preventDefault();this.cancel();}});
    listen(window,'blur',()=>this.cancel());
  }
  select(id) {
    this.view.selected=id;
    for(const el of this.track.querySelectorAll('.chapter-stop')){const active=Number(el.dataset.chapter)===id;el.classList.toggle('selected',active);el.querySelector('.chapter-card').setAttribute('aria-pressed',String(active));}
    const e=this.entries.find(e=>e.chapter_id===id);if(!e){this.detail.innerHTML='';return;}
    const index=this.entries.indexOf(e),name=this.chapters.find(c=>c.id===id)?.name||'Chapter';
    this.detail.innerHTML='<div class="timeline-detail-heading"><h2>'+esc(name)+'</h2><div class="row"><button type="button" data-step="-1" aria-label="Move earlier" '+(!index?'disabled':'')+'>'+icon('back')+'</button><button type="button" data-step="1" aria-label="Move later" '+(index===this.entries.length-1?'disabled':'')+'>'+icon('forward')+'</button><button type="button" data-edit>Edit notes</button></div></div><small>'+esc(e.when||'Undated')+'</small><div class="timeline-notes">'+esc(e.summary||'')+'</div>';
    this.detail.querySelector('[data-edit]').onclick=()=>this.edit(id);
    for(const button of this.detail.querySelectorAll('[data-step]'))button.onclick=()=>this.step(id,Number(button.dataset.step));
  }
  step(id,direction) {
    const index=this.entries.findIndex(e=>e.chapter_id===id);
    if(index+direction<0||index+direction>=this.entries.length)return;
    this.commit(id,this.entries[index+(direction<0?-1:2)]?.chapter_id??null);
  }
  commit(id,before) {
    if(!wouldMove(this.entries.map(e=>e.chapter_id),id,before))return;
    this.view.selected=id;this.view.scroll=this.rail.scrollLeft;this.move(id,before);
  }
  down(e) {
    if(e.button!==0||this.drag)return;
    const stop=e.target.closest('[data-chapter]');if(!stop)return;
    // The rest of a touch card scrolls naturally; its large handle owns dragging.
    if(e.pointerType!=='mouse'&&!e.target.closest('.chapter-grip'))return;
    e.preventDefault();
    this.drag={id:Number(stop.dataset.chapter),pointer:e.pointerId,stop,x:e.clientX,y:e.clientY,startX:e.clientX,startY:e.clientY,active:false};
    this.track.setPointerCapture(e.pointerId);
  }
  pointerMove(e) {
    const d=this.drag;if(!d||e.pointerId!==d.pointer)return;
    d.x=e.clientX;d.y=e.clientY;
    if(!d.active&&Math.hypot(d.x-d.startX,d.y-d.startY)>7){
      d.active=true;this.select(d.id);this.suppressClick=true;
      const rect=d.stop.querySelector('.chapter-card').getBoundingClientRect();
      d.dx=d.startX-rect.left;d.dy=d.startY-rect.top;
      d.ghost=d.stop.querySelector('.chapter-card').cloneNode(true);d.ghost.classList.add('chapter-ghost');d.ghost.style.width=rect.width+'px';d.ghost.setAttribute('aria-hidden','true');d.ghost.inert=true;document.body.append(d.ghost);
      d.stop.classList.add('dragging');this.tick();
    }
  }
  tick() {
    const d=this.drag;if(!d?.active)return;
    const bounds=this.rail.getBoundingClientRect(),now=performance.now(),dt=Math.min(32,now-(this.tickTime||now));this.tickTime=now;
    d.valid=d.y>=bounds.top-24&&d.y<=bounds.bottom+24&&d.x>=bounds.left-48&&d.x<=bounds.right+48;
    if(d.valid){const speed=d.x<bounds.left+48?-Math.min(1,(bounds.left+48-d.x)/48):d.x>bounds.right-48?Math.min(1,(d.x-bounds.right+48)/48):0;this.rail.scrollLeft+=speed*dt*.65;}
    const cards=[...this.track.querySelectorAll('.chapter-stop')].filter(el=>el!==d.stop).map(el=>({id:Number(el.dataset.chapter),rect:el.getBoundingClientRect()}));
    d.before=insertionBefore(cards.map(c=>({id:c.id,center:(c.rect.left+c.rect.right)/2})),d.x);
    const next=cards.find(c=>c.id===d.before),x=next?next.rect.left-11:(cards.at(-1)?.rect.right??bounds.left+20)+11;
    this.marker.hidden=!d.valid;this.marker.style.left=(x-this.track.getBoundingClientRect().left)+'px';
    d.ghost.style.transform='translate('+(d.x-d.dx)+'px,'+(d.y-d.dy)+'px) rotate(2deg)';
    this.raf=requestAnimationFrame(()=>this.tick());
  }
  end(e,cancelled) {
    if(!this.drag||e.pointerId!==this.drag.pointer)return;
    const d=this.drag;
    if(!cancelled&&d.active){d.x=e.clientX;d.y=e.clientY;cancelAnimationFrame(this.raf);this.tick();}
    this.cancel();
    if(!cancelled&&d.active&&d.valid)this.commit(d.id,d.before);
    else if(!cancelled&&!d.active)this.select(d.id);
  }
  cancel() {
    const d=this.drag;this.drag=null;cancelAnimationFrame(this.raf);this.tickTime=0;
    if(d){d.ghost?.remove();d.stop.classList.remove('dragging');if(this.track.hasPointerCapture(d.pointer))this.track.releasePointerCapture(d.pointer);}
    this.marker.hidden=true;
    clearTimeout(this.clickTimer);this.clickTimer=setTimeout(()=>{this.suppressClick=false;},0);
  }
  destroy(){this.abort.abort();this.cancel();clearTimeout(this.clickTimer);}
}
