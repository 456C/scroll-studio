const paths={
characters:'M16 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2M14 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0M17 12a4 4 0 0 1 4 4v5',
locations:'m2 20 7-10 5 7 3-4 5 7M19 6a3 3 0 1 1-6 0 3 3 0 0 1 6 0',
groups:'M9 7a3 3 0 1 1-6 0 3 3 0 0 1 6 0M21 7a3 3 0 1 1-6 0 3 3 0 0 1 6 0M15 13a3 3 0 1 1-6 0 3 3 0 0 1 6 0M1 18v-2a4 4 0 0 1 5-4M23 18v-2a4 4 0 0 0-5-4M6 23v-2a6 6 0 0 1 12 0v2',
timeline:'M3 12h18M6 8v8M12 5v14M18 8v8',
lore:'M7 5a3 3 0 1 1-6 0 3 3 0 0 1 6 0M23 10a3 3 0 1 1-6 0 3 3 0 0 1 6 0M13 20a3 3 0 1 1-6 0 3 3 0 0 1 6 0M7 6l10 3M18 12l-6 6',
images:'M3 3h18v18H3zM3 17l6-7 5 6 3-3 4 5M17 7h.01',
settings:'M5 3v18M12 3v18M19 3v18M2 8h6M9 16h6M16 9h6',
plus:'M12 5v14M5 12h14',close:'m6 6 12 12M6 18 18 6',back:'m14 5-7 7 7 7',
more:'M5 12h.01M12 12h.01M19 12h.01',export:'M12 16V3m-5 5 5-5 5 5M4 15v6h16v-6',
open:'M3 7V4h7l3 3h8v13H3z',copy:'M9 9h12v12H9zM15 9V3H3v12h6',
undo:'m8 4-5 5 5 5M3 9h11a6 6 0 0 1 0 12',redo:'m16 4 5 5-5 5M21 9H10a6 6 0 0 0 0 12',
search:'M15 9a6 6 0 1 1-12 0 6 6 0 0 1 12 0m-2 4 8 8',trash:'M3 6h18M9 6V3h6v3M6 6l1 15h10l1-15M10 10v7M14 10v7',
note:'M4 3h16v18H4zM8 8h8M8 12h8M8 16h5',link:'m10 14 4-4M8 16l-2 2a4 4 0 0 1-5-5l5-5a4 4 0 0 1 5 0M16 8l2-2a4 4 0 0 1 5 5l-5 5a4 4 0 0 1-5 0',
fit:'M3 9V3h6M15 3h6v6M21 15v6h-6M9 21H3v-6',angle:'M12 2v20M2 12h20M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0M17 12a5 9 0 1 1-10 0 5 9 0 0 1 10 0',
check:'m4 12 5 5L20 6',down:'m6 9 6 6 6-6',style:'m12 3 9 5-9 5-9-5zM3 12l9 5 9-5M3 16l9 5 9-5',menu:'M4 6h16M4 12h16M4 18h16'
};
export function icon(name){return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="'+(paths[name]||paths.note)+'"/></svg>';}
export const escape=value=>String(value??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));

