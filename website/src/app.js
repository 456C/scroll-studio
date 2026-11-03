const tabs = [...document.querySelectorAll('[role="tab"]')];
function selectTab(tab, focus = false) {
  for (const item of tabs) {
    const selected = item === tab;
    item.setAttribute('aria-selected', String(selected));
    item.tabIndex = selected ? 0 : -1;
    document.getElementById(item.getAttribute('aria-controls')).hidden = !selected;
  }
  if (focus) tab.focus();
}
for (const tab of tabs) {
  tab.addEventListener('click', () => selectTab(tab));
  tab.addEventListener('keydown', event => {
    const direction = {ArrowRight: 1, ArrowLeft: -1}[event.key];
    let index = tabs.indexOf(tab);
    if (direction) index = (index + direction + tabs.length) % tabs.length;
    else if (event.key === 'Home') index = 0;
    else if (event.key === 'End') index = tabs.length - 1;
    else return;
    event.preventDefault(); selectTab(tabs[index], true);
  });
}

const note = document.getElementById('download-note');
if (note) {
  let lastDownload;
  let dismissed = false;
  for (const link of document.querySelectorAll('a[download][href$=".exe"], a[download][href$=".zip"]')) {
    link.addEventListener('click', event => {
      if (dismissed || event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) return;
      lastDownload = link;
      const anchor = link.closest('.download-box')?.querySelector('.download-links')
        ?? link.closest('.hero-copy')?.querySelector('.download-meta')
        ?? link;
      anchor.after(note);
      note.hidden = false;
      if (event.detail === 0) note.focus();
    });
  }
  note.querySelector('.note-close').addEventListener('click', () => {
    dismissed = true;
    note.hidden = true;
    lastDownload?.focus({preventScroll: true});
  });
}
