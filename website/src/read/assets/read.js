// Progressive enhancement only: every control also works as a plain form or link.
(() => {
  const $ = (s, r = document) => r.querySelector(s), $$ = (s, r = document) => [...r.querySelectorAll(s)];

  // Confirm destructive buttons.
  document.addEventListener('click', e => {
    const b = e.target.closest('[data-confirm]');
    if (b && !confirm(b.dataset.confirm)) e.preventDefault();
  });

  // Like / subscribe without a page reload.
  const send = async form => {
    const r = await fetch(form.action, { method: 'POST', body: new FormData(form), headers: { Accept: 'application/json' }, credentials: 'same-origin' });
    const data = await r.json().catch(() => ({}));
    if (r.status === 401 && data.login) { location.href = data.login; return null; }
    if (!r.ok) throw new Error(data.error || 'Something went wrong');
    return data;
  };
  document.addEventListener('submit', async e => {
    const form = e.target;
    if (!form.matches('.js-like, .js-toggle')) return;
    e.preventDefault();
    const btn = form.querySelector('button');
    btn.disabled = true;
    try {
      const data = await send(form);
      if (!data) return;
      btn.classList.toggle('on', data.on);
      btn.setAttribute('aria-pressed', String(data.on));
      if (form.matches('.js-like')) btn.querySelector('b').textContent = data.count;
      else {
        btn.querySelector('span').textContent = data.on ? form.dataset.on : form.dataset.off;
        btn.querySelector('svg').innerHTML = data.on ? '<path d="m5 12.5 4.5 4.5L19 7.5"/>' : '<path d="M12 5v14M5 12h14"/>';
      }
    } catch (err) { alert(err.message); } finally { btn.disabled = false; }
  });

  // Featured carousel: swipe or dots; no autoplay.
  const slides = $('.slides');
  if (slides) {
    const dots = $$('.dots button');
    const sync = () => {
      const i = Math.round(slides.scrollLeft / slides.clientWidth);
      dots.forEach((d, j) => d.setAttribute('aria-current', String(i === j)));
      $$('.slide', slides).forEach((s, j) => { s.toggleAttribute('aria-hidden', i !== j); s.tabIndex = i === j ? 0 : -1; });
    };
    slides.addEventListener('scroll', () => requestAnimationFrame(sync), { passive: true });
    dots.forEach((d, i) => d.addEventListener('click', () => slides.scrollTo({ left: i * slides.clientWidth, behavior: 'smooth' })));
  }

  // Reader: progress, auto-hiding chrome, drawer, keyboard.
  if (document.body.classList.contains('reading')) {
    const bar = $('.progress span');
    let last = scrollY, ticking = false;
    const update = () => {
      const strip = $('.strip');
      const end = strip ? strip.offsetTop + strip.offsetHeight - innerHeight : document.body.scrollHeight - innerHeight;
      bar.style.width = Math.min(100, Math.max(0, scrollY / Math.max(1, end) * 100)) + '%';
      const down = scrollY > last + 4, up = scrollY < last - 4;
      if (down && scrollY > 120) document.body.classList.add('chrome-off');
      if (up || scrollY < 60 || scrollY >= end) document.body.classList.remove('chrome-off');
      last = scrollY; ticking = false;
    };
    addEventListener('scroll', () => { if (!ticking) { ticking = true; requestAnimationFrame(update); } }, { passive: true });
    $('.strip')?.addEventListener('click', () => document.body.classList.toggle('chrome-off'));
    $('[data-top]')?.addEventListener('click', () => scrollTo({ top: 0 }));
    const drawer = $('#drawer-episodes'), opener = $('[data-drawer]');
    const close = () => { drawer.hidden = true; opener.setAttribute('aria-expanded', 'false'); opener.focus(); };
    opener?.addEventListener('click', () => {
      drawer.hidden = false; opener.setAttribute('aria-expanded', 'true');
      const cur = $('[aria-current=page]', drawer); cur?.scrollIntoView({ block: 'center' }); (cur || $('a', drawer))?.focus();
    });
    drawer?.addEventListener('click', e => { if (e.target === drawer || e.target.closest('[data-close]')) close(); });
    addEventListener('keydown', e => {
      if (e.target.closest('input, textarea, select')) return;
      if (e.key === 'Escape' && !drawer.hidden) close();
      const go = e.key === 'ArrowRight' ? $('a[rel=next]') : e.key === 'ArrowLeft' ? $('.reader-dock a[aria-label="Previous episode"]') : null;
      if (go) location.href = go.href;
    });
  }

  // Episode upload: show chosen files, drag highlight and upload progress.
  const zone = $('[data-dropzone]');
  if (zone) {
    const input = $('input[type=file]', zone), list = $('[data-files]', zone);
    const show = () => {
      const names = [...input.files].map(f => f.name).sort((a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' }));
      list.innerHTML = '';
      names.slice(0, 60).forEach(n => { const li = document.createElement('li'); li.textContent = n; list.append(li); });
      if (names.length > 60) { const li = document.createElement('li'); li.textContent = `+${names.length - 60} more`; list.append(li); }
    };
    input.addEventListener('change', show);
    ['dragenter', 'dragover'].forEach(t => zone.addEventListener(t, () => zone.classList.add('over')));
    ['dragleave', 'drop'].forEach(t => zone.addEventListener(t, () => zone.classList.remove('over')));
    const form = zone.closest('form');
    form.addEventListener('submit', e => {
      e.preventDefault();
      const progress = $('[data-progress]', form), fill = $('span', progress), submit = $('button[type=submit]', form);
      progress.hidden = false; submit.disabled = true;
      const xhr = new XMLHttpRequest();
      xhr.open('POST', form.action);
      xhr.setRequestHeader('Accept', 'application/json');
      xhr.upload.onprogress = ev => { if (ev.lengthComputable) fill.style.width = (ev.loaded / ev.total * 100) + '%'; };
      xhr.onload = () => {
        let data = {}; try { data = JSON.parse(xhr.responseText); } catch {}
        if (xhr.status < 300 && data.redirect) location.href = data.redirect;
        else { alert(data.error || 'Upload failed. Try again.'); submit.disabled = false; progress.hidden = true; fill.style.width = 0; }
      };
      xhr.onerror = () => { alert('Upload failed. Check your connection.'); submit.disabled = false; progress.hidden = true; };
      xhr.send(new FormData(form));
    });
    const schedule = $('[data-schedule]', form);
    const syncSchedule = () => { schedule.hidden = !$('input[name=status][value=scheduled]', form).checked; };
    $$('input[name=status]', form).forEach(r => r.addEventListener('change', syncSchedule)); syncSchedule();
  }

  // Cover preview before upload.
  $$('input[data-preview]').forEach(input => input.addEventListener('change', () => {
    const file = input.files[0], holder = $('.cover-drop'); if (!file || !holder) return;
    const img = document.createElement('img'); img.className = 'cover'; img.alt = ''; img.src = URL.createObjectURL(file);
    holder.replaceChildren(img);
  }));

  // Copy API token.
  $$('[data-copy]').forEach(el => el.addEventListener('click', async () => {
    try { await navigator.clipboard.writeText(el.textContent); el.dataset.copied = '1'; el.textContent = el.textContent; alert('Token copied.'); } catch {}
  }));

  // Close the account menu when clicking elsewhere.
  document.addEventListener('click', e => $$('details.menu[open]').forEach(d => { if (!d.contains(e.target)) d.open = false; }));
})();
