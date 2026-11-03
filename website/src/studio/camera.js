import {escape as esc} from './icons.js';
export function showAngle(api){
  const camera={yaw:30,pitch:0,roll:0,zoom:1,focus:.95};
  const frame=()=>camera.zoom<1.35?'Full body':camera.zoom<2.2?'Waist up':camera.zoom<3.2?'Portrait':'Close-up';
  const prompt=()=>{
    const yaw=((camera.yaw%360)+360)%360,side=yaw>180?'left':'right',turn=Math.min(yaw,360-yaw);
    const view=turn<15?'front view':turn<65?'front three-quarter view':turn<115?'side profile':turn<165?'rear three-quarter view':'back view';
    const elevation=camera.pitch>60?'overhead, looking down':camera.pitch>15?'high angle, looking down':camera.pitch< -60?'extreme low angle, looking up':camera.pitch< -15?'low angle, looking up':'eye level';
    return 'Camera and framing: '+view+' from the '+side+', '+elevation+'. Orbit '+Math.round(camera.yaw)+' degrees from the front; elevation '+Math.round(camera.pitch)+' degrees; camera roll '+Math.round(camera.roll)+' degrees clockwise. '+frame()+' composition, centered at '+Math.round(camera.focus/1.9*100)+'% of the character’s height. Preserve the character’s proportions and design. Do not include a mannequin, rig, compass, or angle guides in the image.';
  };
  if(!api.character){api.modal('Scene angle','<select id="scene-angle" aria-label="Camera angle">'+['Eye-level wide shot','High-angle establishing shot','Low angle looking up','Street-level perspective','Bird’s-eye view','Interior corner view','Close detail'].map(t=>'<option>'+t+'</option>').join('')+'</select><textarea id="angle-scene" placeholder="Scene details…" rows="4"></textarea>','<button id="copy-angle" class="primary">Copy prompt</button>');document.querySelector('#copy-angle').onclick=()=>api.copy(api.prompt()+'\n\nCAMERA\n'+document.querySelector('#scene-angle').value+'\n'+document.querySelector('#angle-scene').value);return;}
  api.modal('Character angle','<svg class="angle-stage" id="angle-stage" viewBox="0 0 360 290" aria-label="Drag to orbit the character"></svg><div class="row wrap">'+[['Front',0],['¾',45],['Side',90],['Back',180]].map(([label,v])=>'<button data-yaw="'+v+'">'+label+'</button>').join('')+'</div><div class="angle-controls">'+[['yaw','Orbit',-180,180,1],['pitch','Tilt',-85,85,1],['roll','Roll',-90,90,1],['zoom','Frame',.6,4,.05],['focus','Focus',0,1.9,.05]].map(([key,label,min,max,step])=>'<label>'+label+' <span id="value-'+key+'"></span><input type="range" data-angle="'+key+'" min="'+min+'" max="'+max+'" step="'+step+'" value="'+camera[key]+'"></label>').join('')+'</div><p class="plain-text" id="angle-prompt"></p>','<button id="copy-angle" class="primary">Copy prompt</button>');
  const svg=document.querySelector('#angle-stage'),rad=Math.PI/180;
  function project(p){
    const y=p[1]-camera.focus,x=p[0]*Math.cos(camera.yaw*rad)+p[2]*Math.sin(camera.yaw*rad),z=-p[0]*Math.sin(camera.yaw*rad)+p[2]*Math.cos(camera.yaw*rad);
    const yp=y*Math.cos(camera.pitch*rad)-z*Math.sin(camera.pitch*rad),zp=y*Math.sin(camera.pitch*rad)+z*Math.cos(camera.pitch*rad),xp=x*Math.cos(camera.roll*rad)-yp*Math.sin(camera.roll*rad),yr=x*Math.sin(camera.roll*rad)+yp*Math.cos(camera.roll*rad),scale=112*camera.zoom*3.8/Math.max(.5,3.8-zp);
    return{x:180+xp*scale,y:145-yr*scale,z:zp,scale};
  }
  function draw(){
    let html='<defs><clipPath id="angle-clip"><rect width="360" height="290" rx="10"/></clipPath></defs><g clip-path="url(#angle-clip)"><ellipse cx="180" cy="145" rx="150" ry="42" fill="none" stroke="#7bb8f166"/><ellipse cx="180" cy="145" rx="66" ry="132" fill="none" stroke="#c2ae8877"/><circle cx="180" cy="145" r="128" fill="none" stroke="#6e96b555"/>';
    for(let i=0;i<36;i++){const a=i*Math.PI/18,r=i%3?147:142;html+='<path d="M'+(180+Math.cos(a)*r)+' '+(145+Math.sin(a)*r*.28)+' L'+(180+Math.cos(a)*151)+' '+(145+Math.sin(a)*42.3)+'" stroke="#7f9cb5"/>';}
    const joints=[[0,1.65,0],[0,1.42,0],[0,.95,0],[-.28,1.4,0],[-.43,1.03,.05],[-.43,.73,.08],[.28,1.4,0],[.43,1.03,.05],[.43,.73,.08],[-.14,.91,0],[-.18,.47,.04],[-.18,.04,.08],[.14,.91,0],[.18,.47,.04],[.18,.04,.08]];
    const segments=[[1,2,26],[1,3,16],[3,4,15],[4,5,12],[1,6,16],[6,7,15],[7,8,12],[2,9,18],[9,10,19],[10,11,15],[2,12,18],[12,13,19],[13,14,15]];
    const points=joints.map(project);
    const shapes=segments.map(([a,b,width])=>({z:(points[a].z+points[b].z)/2,html:'<path d="M'+points[a].x+' '+points[a].y+' L'+points[b].x+' '+points[b].y+'" stroke="'+(points[a].z>0?'#c4d4e3':'#8fa6ba')+'" stroke-width="'+width*camera.zoom+'" stroke-linecap="round"/>'}));
    const head=points[0];shapes.push({z:head.z,html:'<ellipse cx="'+head.x+'" cy="'+head.y+'" rx="'+head.scale*.105+'" ry="'+head.scale*.14+'" fill="#d6e1e9"/>'});
    const nose=project([0,1.66,.13]);shapes.push({z:nose.z,html:'<circle cx="'+nose.x+'" cy="'+nose.y+'" r="'+3*camera.zoom+'" fill="#445665"/>'});
    shapes.sort((a,b)=>a.z-b.z);html+=shapes.map(s=>s.html).join('')+'</g>';svg.innerHTML=html;
    document.querySelector('#angle-prompt').textContent=prompt();
    for(const key of ['yaw','pitch','roll','zoom','focus']){document.querySelector('[data-angle="'+key+'"]').value=camera[key];document.querySelector('#value-'+key).textContent=key==='zoom'?frame():key==='focus'?Math.round(camera.focus/1.9*100)+'%':Math.round(camera[key])+'°';}
  }
  document.querySelectorAll('[data-angle]').forEach(input=>input.oninput=()=>{camera[input.dataset.angle]=Number(input.value);draw();});
  document.querySelectorAll('[data-yaw]').forEach(b=>b.onclick=()=>{camera.yaw=Number(b.dataset.yaw);draw();});
  let drag=null;
  svg.addEventListener('pointerdown',e=>{svg.setPointerCapture(e.pointerId);drag={x:e.clientX,y:e.clientY,yaw:camera.yaw,pitch:camera.pitch};});
  svg.addEventListener('pointermove',e=>{if(!drag)return;camera.yaw=((drag.yaw+(e.clientX-drag.x)*.7+540)%360)-180;camera.pitch=Math.max(-85,Math.min(85,drag.pitch+(e.clientY-drag.y)*.6));draw();});
  svg.addEventListener('pointerup',()=>drag=null);svg.addEventListener('pointercancel',()=>drag=null);
  svg.addEventListener('wheel',e=>{e.preventDefault();camera.zoom=Math.max(.6,Math.min(4,camera.zoom*Math.exp(-e.deltaY*.002)));draw();},{passive:false});
  document.querySelector('#copy-angle').onclick=()=>api.copy(api.prompt()+'\n\n'+prompt());
  draw();
}

