// Browsing never requests the software keyboard. Only a deliberate edit does.
export const touchEditing=()=>matchMedia('(pointer:coarse), (max-width:680px)').matches;
export function focusForTyping(input,{select=false,explicit=false}={}) {
  if(!input || (!explicit && touchEditing()))return;
  input.focus({preventScroll:true});
  if(select)input.select();
}

export function prepareEditors(root) {
  for(const input of root.querySelectorAll('textarea:not([readonly])')) {
    if(input.closest('.text-editor'))continue;
    const wrap=document.createElement('div');wrap.className='text-editor';
    const toggle=document.createElement('button');toggle.type='button';toggle.className='text-edit-toggle ghost';
    input.before(wrap);wrap.append(toggle,input);
    const setEditing=active=>{
      input.readOnly=!active;toggle.textContent=active?'Done':'Edit';
      toggle.setAttribute('aria-label',(active?'Finish editing ':'Edit ')+(input.getAttribute('aria-label')||'text'));
      wrap.classList.toggle('editing',active);
      if(active)focusForTyping(input,{explicit:true});else input.blur();
    };
    toggle.addEventListener('click',()=>setEditing(input.readOnly));
    input.addEventListener('keydown',e=>{if(e.key==='Escape'){e.stopPropagation();setEditing(false);toggle.focus({preventScroll:true});}});
    input.addEventListener('finish-editing',()=>setEditing(false));
    setEditing(false);
  }
}

export function installKeyboardDismissal() {
  const done=document.createElement('button');done.className='keyboard-done';done.type='button';done.textContent='Done';done.hidden=true;
  document.body.append(done);
  const editable=e=>e?.matches('textarea:not([readonly]),input:not([readonly]):not([type=checkbox]):not([type=color]):not([type=range]):not([type=file]):not([type=hidden])');
  let active=null;
  const update=()=>{
    active=editable(document.activeElement)?document.activeElement:null;
    done.hidden=!active||!touchEditing();
    const v=window.visualViewport;
    done.style.top=((v?.offsetTop||0)+8)+'px';
  };
  done.addEventListener('pointerdown',e=>e.preventDefault());
  done.addEventListener('click',()=>{const field=active;field?.dispatchEvent(new Event('finish-editing'));field?.blur();update();});
  document.addEventListener('focusin',update);
  document.addEventListener('focusout',()=>queueMicrotask(update));
  document.addEventListener('pointerdown',e=>{
    if(touchEditing()&&editable(document.activeElement)&&!editable(e.target)&&!e.target.closest('.text-editor,.keyboard-done'))document.activeElement.blur();
  },{capture:true});
  document.addEventListener('keydown',e=>{if(e.key==='Enter'&&e.target.matches('input[type=search]')&&!e.isComposing){e.preventDefault();e.target.blur();}});
  window.visualViewport?.addEventListener('resize',update);
  window.visualViewport?.addEventListener('scroll',update);
}
