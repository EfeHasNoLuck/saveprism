(() => {
  if (!window.__fetchPatched) {
    const of = window.fetch.bind(window);
    window.__ipcLog = [];
    window.fetch = async (url, init) => {
      const u = String(url);
      if (u.startsWith('http://ipc.localhost/')) {
        const cmd = decodeURIComponent(u.slice('http://ipc.localhost/'.length));
        window.__ipcLog.push(cmd);
        const ok = (v) => new Response(JSON.stringify(v), { headers: { 'Tauri-Response': 'ok', 'content-type': 'application/json' } });
        if (cmd === 'plugin:dialog|open') return ok(window.__nextOpen ?? null);
        if (cmd === 'plugin:dialog|save') return ok(window.__nextSave ?? null);
        if (cmd === 'save') {
          let body = init && init.body; let t = '';
          try { t = typeof body === 'string' ? body : new TextDecoder().decode(body); } catch (e) { t = String(body); }
          window.__lastSaveBody = t;
          if (!t.includes('scratchpad')) return new Response(JSON.stringify('BLOCKED by test harness'), { headers: { 'Tauri-Response': 'error', 'content-type': 'application/json' } });
        }
      }
      return of(url, init);
    };
    window.__fetchPatched = true;
  }
  return 'patched';
})()
