import {icon,escape as esc} from './icons.js';
import {targets,connect,hex} from './model.js';
const clamp=(n,a,b)=>Math.max(a,Math.min(b,n));
export class LoreBoard {
  constructor(root,api){
    this.root=root;this.api=api;this.view=api.view;this.nodes=new Map();this.pointers=new Map();this.abort=new AbortController();this.frame=0;this.link=null;this.drag=null;this.hold=null;this.longPressed=false;
    root.innerHTML='<div class="board-plane"><svg class="board-edges" aria-label="Connections"></svg><div class="board-cards"></div></div><div class="board-empty"></div>';
    this.plane=root.querySelector('.board-plane');this.svg=root.querySelector('.board-edges');this.cards=root.querySelector('.board-cards');this.empty=root.querySelector('.board-empty');
    const on=(name,fn,options={})=>root.addEventListener(name,fn,{...options,signal:this.abort.signal});
    on('pointerdown',e=>this.down(e));on('pointermove',e=>this.move(e));on('pointerup',e=>this.up(e));on('pointercancel',e=>this.cancel(e));on('contextmenu',e=>this.context(e));
    on('wheel',e=>this.wheel(e),{passive:false});
    on('dblclick',e=>{const node=e.target.closest('[data-card]');if(node){const c=this.api.story().lore_board.cards.find(c=>c.id===Number(node.dataset.card));this.api.open(c.target);}});
    on('keydown',e=>{if(e.key==='Escape')this.cancelLink();if(e.key==='Delete'&&this.view.selected){e.preventDefault();this.api.remove(this.view.selected);}});
    this.resize=new ResizeObserver(()=>{if(!this.view.ready&&this.api.story().lore_board.cards.length)this.fit();else this.schedule();});this.resize.observe(root);
  }
  destroy(){this.abort.abort();this.resize.disconnect();clearTimeout(this.hold);cancelAnimationFrame(this.frame);}
  point(e){const r=this.root.getBoundingClientRect();return{x:e.clientX-r.left,y:e.clientY-r.top};}
  world(p){return[(p.x-this.view.x)/this.view.zoom,(p.y-this.view.y)/this.view.zoom];}
  center(){return this.world({x:this.root.clientWidth/2-104*this.view.zoom,y:this.root.clientHeight/2-82*this.view.zoom});}
  height(card){return this.targetMap?.get(card.target)?.type==='note'?164:236;}
  bounds(card){return{x:card.position[0],y:card.position[1],w:208,h:this.height(card)};}
  fit(){
    const list=this.api.story().lore_board.cards;if(!list.length){this.view.x=0;this.view.y=0;this.view.zoom=1;this.schedule();return;}
    this.targetMap=targets(this.api.story());let x0=Infinity,y0=Infinity,x1=-Infinity,y1=-Infinity;
    for(const c of list){x0=Math.min(x0,c.position[0]);y0=Math.min(y0,c.position[1]);x1=Math.max(x1,c.position[0]+208);y1=Math.max(y1,c.position[1]+this.height(c));}
    const z=clamp(Math.min((this.root.clientWidth-80)/(x1-x0),(this.root.clientHeight-80)/(y1-y0)),.25,1.25);
    this.view.zoom=z;this.view.x=(this.root.clientWidth-(x0+x1)*z)/2;this.view.y=(this.root.clientHeight-(y0+y1)*z)/2;this.view.ready=true;this.schedule();
  }
  focus(id){const c=this.api.story().lore_board.cards.find(c=>c.id===id);if(!c)return;this.view.x=this.root.clientWidth/2-(c.position[0]+104)*this.view.zoom;this.view.y=this.root.clientHeight/2-(c.position[1]+this.height(c)/2)*this.view.zoom;this.view.ready=true;this.schedule();}
  refresh(){this.targetMap=targets(this.api.story());this.schedule();}
  schedule(){if(!this.frame)this.frame=requestAnimationFrame(()=>{this.frame=0;this.paint();});}
  paint(){
    const s=this.api.story(),v=this.view,w=this.root.clientWidth,h=this.root.clientHeight;
    this.targetMap??=targets(s);
    this.plane.style.transform='translate('+v.x+'px,'+v.y+'px) scale('+v.zoom+')';
    this.root.style.backgroundSize=32*v.zoom+'px '+32*v.zoom+'px';this.root.style.backgroundPosition=v.x+'px '+v.y+'px';
    const zoom=document.querySelector('.board-zoom');if(zoom)zoom.textContent=Math.round(v.zoom*100)+'%';
    const needed=new Set(),rects=new Map();
    for(const c of s.lore_board.cards){
      const r=this.bounds(c);rects.set(c.id,r);
      if((r.x+r.w)*v.zoom+v.x< -100 || r.x*v.zoom+v.x>w+100 || (r.y+r.h)*v.zoom+v.y< -100 || r.y*v.zoom+v.y>h+100)continue;
      const t=this.targetMap.get(c.target);if(!t)continue;needed.add(c.id);
      let node=this.nodes.get(c.id);
      if(!node){node=document.createElement('article');node.dataset.card=c.id;this.cards.append(node);this.nodes.set(c.id,node);}
      node.className='board-card'+(t.type==='note'?' note':'')+(c.id===v.selected?' selected':'');
      node.style.left=r.x+'px';node.style.top=r.y+'px';node.style.setProperty('--card-color',hex(c.color));
      const text=t.type==='note'?t.text:c.note,signature=JSON.stringify([t.name,t.portrait,text,t.type]);
      if(node.dataset.signature!==signature){
        node.innerHTML='<small>'+esc(t.label)+'</small><h3>'+esc(t.name||'Untitled')+'</h3>'+(t.type==='note'?'':'<div class="board-art">'+(t.portrait?'<img src="'+this.api.imageURL(t.portrait)+'" alt="" loading="lazy" decoding="async">':icon(t.type))+'</div>')+'<p>'+esc((text||'').slice(0,260))+'</p><button class="board-port" aria-label="Connect '+esc(t.name)+'" title="Connect"></button>';
        node.dataset.signature=signature;
      }
    }
    for(const [id,node] of this.nodes)if(!needed.has(id)){node.remove();this.nodes.delete(id);}
    let paths='';
    for(const e of s.lore_board.connections){
      const a=rects.get(e.from),b=rects.get(e.to);if(!a||!b)continue;
      if(Math.max(a.x+a.w,b.x+b.w)*v.zoom+v.x<0||Math.min(a.x,b.x)*v.zoom+v.x>w||Math.max(a.y+a.h,b.y+b.h)*v.zoom+v.y<0||Math.min(a.y,b.y)*v.zoom+v.y>h)continue;
      const curve=this.curve(a,b),label=(e.label||'•').slice(0,30),width=Math.max(24,label.length*6+16),selected=e.id===v.edge;
      paths+='<g data-edge="'+e.id+'"><path d="'+curve.path+'" stroke="'+(selected?'#7bb8f1':hex(e.color))+'"'+(selected?' style="stroke-width:3"':'')+'/><path class="edge-hit" d="'+curve.path+'"/><rect x="'+(curve.mx-width/2)+'" y="'+(curve.my-13)+'" width="'+width+'" height="26" rx="5" fill="'+(selected?'#354454':'#282a2e')+'" style="pointer-events:auto"/><text class="edge-label" text-anchor="middle" x="'+curve.mx+'" y="'+(curve.my+4)+'">'+esc(label)+'</text></g>';
    }
    if(this.link?.from && this.link.pointer){
      const from=rects.get(this.link.from),p=this.world(this.link.pointer);
      if(from){const curve=this.curve(from,{x:p[0],y:p[1],w:1,h:1});paths+='<path d="'+curve.path+'" stroke="#7bb8f1" stroke-dasharray="5 4"/>';}
    }
    this.svg.innerHTML=paths;
    this.root.classList.toggle('board-linking',!!this.link);
    if(!s.lore_board.cards.length){
      if(!this.empty.firstChild){this.empty.innerHTML='<div class="empty-inner"><div class="empty-mark">'+icon('lore')+'</div><div class="row"><button class="primary" data-action="addNote">'+icon('plus')+'Note</button><button data-action="pinCard">'+icon('characters')+'Card</button></div></div>';}
    }else this.empty.innerHTML='';
  }
  curve(a,b){
    const ax=a.x+a.w/2,ay=a.y+a.h/2,bx=b.x+b.w/2,by=b.y+b.h/2,dx=bx-ax,dy=by-ay;
    const boundary=(r,x,y,vx,vy)=>{const t=Math.min(r.w/2/Math.max(.001,Math.abs(vx)),r.h/2/Math.max(.001,Math.abs(vy)));return[x+vx*t,y+vy*t];};
    const [sx,sy]=boundary(a,ax,ay,dx,dy),[ex,ey]=boundary(b,bx,by,-dx,-dy),sag=Math.min(48,Math.hypot(ex-sx,ey-sy)*.1),c1x=sx+(ex-sx)*.33,c2x=sx+(ex-sx)*.67,c1y=sy+(ey-sy)*.33+sag,c2y=sy+(ey-sy)*.67+sag;
    return{path:'M'+sx+' '+sy+' C'+c1x+' '+c1y+','+c2x+' '+c2y+','+ex+' '+ey,mx:(sx+3*c1x+3*c2x+ex)/8,my:(sy+3*c1y+3*c2y+ey)/8};
  }
  startLink(from=null){this.link={from,pointer:{x:this.root.clientWidth/2,y:this.root.clientHeight/2}};this.view.edge=null;this.schedule();}
  cancelLink(){this.link=null;this.schedule();}
  down(e){
    if(e.button===2)return;
    if(e.target.closest('[data-action]'))return;
    const p=this.point(e);this.pointers.set(e.pointerId,p);this.root.setPointerCapture(e.pointerId);this.longPressed=false;
    if(this.pointers.size===2){
      clearTimeout(this.hold);this.drag=null;this.link=null;
      const [a,b]=[...this.pointers.values()],center={x:(a.x+b.x)/2,y:(a.y+b.y)/2};
      this.pinch={distance:Math.hypot(a.x-b.x,a.y-b.y),zoom:this.view.zoom,world:this.world(center)};return;
    }
    const node=e.target.closest('[data-card]'),edge=e.target.closest('[data-edge]'),card=node?this.api.story().lore_board.cards.find(c=>c.id===Number(node.dataset.card)):null;
    if(e.target.closest('.board-port')){this.startLink(card.id);this.drag={type:'link',start:p,moved:false};}
    else if(this.link){this.drag={type:'link',start:p,moved:false};}
    else if(card){this.drag={type:'card',id:card.id,start:p,position:[...card.position],moved:false};this.view.selected=card.id;this.view.edge=null;}
    else if(edge){this.drag={type:'edge',id:Number(edge.dataset.edge),start:p,moved:false};}
    else{this.drag={type:'pan',start:p,initial:{x:this.view.x,y:this.view.y},moved:false};}
    if(e.button===1)this.drag={type:'pan',start:p,initial:{x:this.view.x,y:this.view.y},moved:false};
    if(e.pointerType==='touch')this.hold=setTimeout(()=>{if(this.drag&&!this.drag.moved){this.longPressed=true;this.contextAt(p,{x:e.clientX,y:e.clientY},card?.id);}},550);
    this.schedule();
  }
  move(e){
    const p=this.point(e);
    if(this.link){this.link.pointer=p;this.schedule();}
    if(!this.pointers.has(e.pointerId))return;this.pointers.set(e.pointerId,p);
    if(this.pointers.size===2 && this.pinch){
      const [a,b]=[...this.pointers.values()],center={x:(a.x+b.x)/2,y:(a.y+b.y)/2};
      this.view.zoom=clamp(this.pinch.zoom*Math.hypot(a.x-b.x,a.y-b.y)/Math.max(1,this.pinch.distance),.25,2.25);
      this.view.x=center.x-this.pinch.world[0]*this.view.zoom;this.view.y=center.y-this.pinch.world[1]*this.view.zoom;this.view.ready=true;this.schedule();return;
    }
    const d=this.drag;if(!d||this.longPressed)return;
    const dx=p.x-d.start.x,dy=p.y-d.start.y;
    if(!d.moved && Math.hypot(dx,dy)<5)return;
    clearTimeout(this.hold);
    if(!d.moved&&d.type==='card')this.api.checkpoint();d.moved=true;
    if(d.type==='pan'){this.view.x=d.initial.x+dx;this.view.y=d.initial.y+dy;this.view.ready=true;}
    if(d.type==='card'){
      const c=this.api.story().lore_board.cards.find(c=>c.id===d.id);
      if(c)c.position=[clamp(d.position[0]+dx/this.view.zoom,-1e6,1e6),clamp(d.position[1]+dy/this.view.zoom,-1e6,1e6)];
    }
    this.schedule();
  }
  up(e){
    clearTimeout(this.hold);this.pointers.delete(e.pointerId);
    if(this.pinch){if(!this.pointers.size)this.pinch=null;this.drag=null;return;}
    const d=this.drag;this.drag=null;if(!d||this.longPressed)return;
    if(d.type==='card'){if(d.moved)this.api.changed();else this.api.inspect();}
    if(d.type==='edge'&&!d.moved){this.view.edge=d.id;this.view.selected=null;this.api.inspect();}
    if(d.type==='link'){
      const point=this.world(this.point(e)),destination=[...this.api.story().lore_board.cards].reverse().find(c=>point[0]>=c.position[0]&&point[0]<=c.position[0]+208&&point[1]>=c.position[1]&&point[1]<=c.position[1]+this.height(c));
      if(destination){
        if(!this.link.from){this.link.from=destination.id;}
        else if(destination.id!==this.link.from){this.api.checkpoint();const edge=connect(this.api.story(),this.link.from,destination.id);this.view.edge=edge?.id;this.view.selected=null;this.link=null;this.api.changed();this.api.inspect();}
      }
    }
    if(d.type==='pan'&&!d.moved){this.view.selected=null;this.view.edge=null;this.api.inspect();}
    this.schedule();
  }
  cancel(e){clearTimeout(this.hold);this.pointers.delete(e.pointerId);if(this.drag?.moved&&this.drag.type==='card')this.api.changed();this.drag=null;this.pinch=null;this.schedule();}
  wheel(e){
    e.preventDefault();const p=this.point(e);
    if(e.ctrlKey||e.metaKey){const point=this.world(p);this.view.zoom=clamp(this.view.zoom*Math.exp(-e.deltaY*.003),.25,2.25);this.view.x=p.x-point[0]*this.view.zoom;this.view.y=p.y-point[1]*this.view.zoom;}
    else {this.view.x-=e.deltaX;this.view.y-=e.deltaY;}
    this.view.ready=true;this.schedule();
  }
  context(e){e.preventDefault();clearTimeout(this.hold);const c=e.target.closest('[data-card]');this.contextAt(this.point(e),{x:e.clientX,y:e.clientY},c?Number(c.dataset.card):null);}
  contextAt(p,screen,id){
    const c=this.api.story().lore_board.cards.find(c=>c.id===id);
    if(c){this.view.selected=c.id;this.view.edge=null;this.api.menu([{label:'Open / edit',icon:'open',run:()=>this.api.open(c.target)},{label:'Connect to…',icon:'link',run:()=>this.startLink(c.id)},{label:'Remove from board',icon:'trash',run:()=>this.api.remove(c.id)}],screen.x,screen.y);}
    else{const position=this.world(p);this.api.menu([{label:'Add note',icon:'note',run:()=>this.api.addNote(position)},{label:'Add card',icon:'characters',run:()=>this.api.pin(position)},{label:'Fit board',icon:'fit',run:()=>this.fit()}],screen.x,screen.y);}
    this.schedule();
  }
}

