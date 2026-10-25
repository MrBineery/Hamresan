<script>
  import { onMount } from 'svelte';
  import { fly, slide, fade } from 'svelte/transition';
  import { flip } from 'svelte/animate';

  let lang = $state(localStorage.getItem('hr-lang') || 'fa');
  let theme = $state(localStorage.getItem('hr-theme') || 'dark');
  let currentPin = $state('');
  let networkUrl = $state(''); // New state for the correct network URL
  let isLocalhost = $state(false);
  let files = $state([]);
  let clipboardText = $state('');
  let toastMsg = $state('');
  let toastType = $state('');
  let toastTimeout;
  let isUploading = $state(false);
  let uploadPercent = $state(0);
  let uploadName = $state('');
  let isDragging = $state(false);
  let showLogin = $state(false);
  let pinInput = $state('');
  let loginError = $state('');

  const T = {
    fa: { 
      copied: "کپی شد", uploaded: "آپلود کامل شد", error: "خطا", 
      delete: "حذف", download: "دانلود", confirmDelete: "این آیتم حذف شود؟", 
      pinCopied: "کد کپی شد", send: "ارسال", clear: "پاک کردن",
      emptyTitle: "هنوز فایلی نیست", emptySub: "برای شروع، فایل‌ها را اینجا رها کنید یا انتخاب کنید." 
    },
    en: { 
      copied: "Copied", uploaded: "Upload complete", error: "Error", 
      delete: "Delete", download: "Download", confirmDelete: "Delete this item?", 
      pinCopied: "PIN copied", send: "Send", clear: "Clear",
      emptyTitle: "No files yet", emptySub: "Drop files here or click to browse to get started." 
    }
  };
  let t = $derived(T[lang]);

  $effect(() => { document.documentElement.setAttribute('data-theme', theme); localStorage.setItem('hr-theme', theme); });
  $effect(() => { document.documentElement.lang = lang; document.documentElement.dir = lang === 'fa' ? 'rtl' : 'ltr'; localStorage.setItem('hr-lang', lang); });

  function showToast(msg, type = 'success') { toastMsg = msg; toastType = type; clearTimeout(toastTimeout); toastTimeout = setTimeout(() => { toastMsg = ''; }, 3000); }
  function formatSize(bytes) { if (bytes === 0) return '0 B'; const k = 1024, sizes = ['B', 'KB', 'MB', 'GB', 'TB']; const i = Math.floor(Math.log(bytes) / Math.log(k)); return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i]; }
  function formatDate(ts) { if (!ts) return ''; const d = new Date(ts * 1000); return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,'0')}-${String(d.getDate()).padStart(2,'0')}`; }
  function formatTime(ts) { if (!ts) return ''; const d = new Date(ts * 1000); return `${String(d.getHours()).padStart(2,'0')}:${String(d.getMinutes()).padStart(2,'0')}`; }

  async function checkAuth() { try { const r = await fetch('/api/auth-status'); const data = await r.json(); if (data.needs_pin && !data.authenticated) { showLogin = true; } else { showLogin = false; initApp(); } } catch { showLogin = true; } }
  async function doLogin() { const pin = pinInput.replace(/\D/g, ''); if (pin.length < 6) { loginError = lang === 'fa' ? 'کد ۶ رقمی وارد کنید' : 'Enter 6-digit PIN'; return; } try { const r = await fetch('/api/login', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ pin }), credentials: 'same-origin' }); if (r.ok) { showLogin = false; initApp(); } else { loginError = 'کد اشتباه است'; pinInput = ''; } } catch { loginError = 'Network error'; } }
  
  async function loadInfo() { 
    try { 
      const r = await fetch('/api/info'); 
      const data = await r.json(); 
      currentPin = data.pin || ''; 
      // Use the real network URL from the backend, fallback to current origin
      networkUrl = data.url || window.location.origin; 
    } catch {
      networkUrl = window.location.origin;
    } 
  }
  
  async function loadFiles() { try { const r = await fetch('/api/files', { credentials: 'same-origin' }); if (r.status === 401) { checkAuth(); return; } const data = await r.json(); files = data.files || []; } catch {} }
  async function loadClipboard() { try { const r = await fetch('/api/clipboard', { credentials: 'same-origin' }); if (r.status === 401) return; const data = await r.json(); if (data.text && document.activeElement?.id !== 'clipInput') { clipboardText = data.text; } } catch {} }
  async function sendClipboard() { try { await fetch('/api/clipboard', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text: clipboardText }), credentials: 'same-origin' }); showToast('ارسال شد'); } catch {} }
  async function clearClipboard() { clipboardText = ''; await fetch('/api/clipboard', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ text: '' }), credentials: 'same-origin' }); }
  async function deleteFile(name) { if (!confirm(t.confirmDelete)) return; await fetch('/api/files/' + name, { method: 'DELETE', credentials: 'same-origin' }); loadFiles(); }
  function copyPin() { if (!currentPin) return; navigator.clipboard.writeText(currentPin.replace(/\s/g, '')); showToast(t.pinCopied); }

  async function uploadFiles(fileList) {
    if (!fileList.length) return;
    isUploading = true;
    for (let i = 0; i < fileList.length; i++) {
      const file = fileList[i];
      uploadName = `(${i+1}/${fileList.length}) ${file.name}`;
      await new Promise((resolve) => {
        const fd = new FormData();
        fd.append('file', file, file.webkitRelativePath || file.name);
        const xhr = new XMLHttpRequest();
        xhr.open('POST', '/api/upload');
        xhr.withCredentials = true;
        xhr.upload.onprogress = (e) => { if (e.lengthComputable) { uploadPercent = Math.round((e.loaded / e.total) * 100); } };
        xhr.onload = () => resolve();
        xhr.onerror = () => resolve();
        xhr.send(fd);
      });
    }
    isUploading = false; loadFiles(); showToast(t.uploaded);
  }

  function handleDrop(e) { isDragging = false; if (e.dataTransfer.files.length) uploadFiles(e.dataTransfer.files); }
  function handleFileSelect(e) { uploadFiles(e.target.files); e.target.value = ''; }
  function initApp() { loadInfo(); loadFiles(); loadClipboard(); isLocalhost = window.location.hostname === 'localhost' || window.location.hostname === '127.0.0.1'; setInterval(() => { loadFiles(); loadClipboard(); }, 3000); }
  
  onMount(() => { checkAuth(); });
</script>

{#if showLogin}
  <div class="login-overlay show" transition:fade={{ duration: 200 }}>
    <div class="login-card" transition:fly={{ y: 20, duration: 300 }}>
      <div style="font-size: 40px; margin-bottom: 16px;">🔐</div>
      <h2>{lang === 'fa' ? 'ورود' : 'Login'}</h2>
      <p>{lang === 'fa' ? 'کد را وارد کنید' : 'Enter PIN'}</p>
      <input type="tel" bind:value={pinInput} class="pin-input" maxlength="6" placeholder="------" onkeydown={(e) => e.key === 'Enter' && doLogin()}>
      <button class="btn-primary" style="width: 100%;" onclick={doLogin}>ورود</button>
      <div style="color: var(--danger); font-size: 13px; margin-top: 12px; height: 18px;">{loginError}</div>
    </div>
  </div>
{/if}

<header>
  <div class="header-left">
    <div class="logo-icon">📡</div>
    <div class="brand">
      <h1>هم‌رسان <span style="font-size:12px; color:var(--text-muted); font-weight:400;">Hamresan</span></h1>
      <div class="node-info"><span class="node-dot"></span> P2P Active</div>
    </div>
  </div>
  <div class="header-actions">
    <button class="icon-btn" onclick={() => theme = theme === 'dark' ? 'light' : 'dark'}>{theme === 'dark' ? '☀️' : '🌙'}</button>
    <button class="icon-btn text" onclick={() => lang = lang === 'fa' ? 'en' : 'fa'}>{lang === 'fa' ? 'EN' : 'FA'}</button>
  </div>
</header>

<div class="layout">
  <aside class="sidebar">
    <div class="card" transition:fly={{ y: 10, duration: 300 }}>
      <div class="card-header"><span>📱 اسکن</span></div>
      <div class="qr-container"><img src="/api/qr" alt="QR"></div>
      <!-- Network URL is now loaded from the backend -->
      <div class="pin-hint" style="margin-bottom: 12px;"><span class="mono" style="color:var(--primary-hi);">{networkUrl}</span></div>
    </div>
    
    {#if isLocalhost && currentPin}
      <div class="card pin-card" transition:fly={{ y: 10, duration: 300, delay: 100 }}>
        <div class="card-header"><span>🔑 کد امنیتی</span></div>
        <div class="pin-display"><div class="pin-digits">{currentPin}</div></div>
        <div class="pin-actions"><button class="btn-outline" onclick={copyPin}>📋 کپی</button></div>
      </div>
    {/if}
  </aside>

  <main class="main-content">
    <div class="dropzone" role="button" tabindex="0" class:drag={isDragging} 
         ondragover={(e) => { e.preventDefault(); isDragging = true; }} 
         ondragleave={() => isDragging = false} 
         ondrop={handleDrop}
         transition:fly={{ y: 10, duration: 300, delay: 150 }}>
      <div class="dropzone-icon">☁️</div>
      <h2>فایل‌ها را اینجا رها کنید</h2>
      <div style="display: flex; gap: 12px; margin-top: 8px;">
        <label class="btn-primary">📄 انتخاب فایل <input type="file" multiple hidden onchange={handleFileSelect}></label>
        <label class="btn-outline" style="background: var(--surface); cursor: pointer;">📁 انتخاب پوشه <input type="file" webkitdirectory multiple hidden onchange={handleFileSelect}></label>
      </div>
    </div>

    {#if isUploading}
      <div class="card progress-card active" transition:slide={{ duration: 200 }}>
        <div class="progress-header"><span>{uploadName}</span><span class="mono">{uploadPercent}%</span></div>
        <div class="progress-bar"><div class="progress-fill" style="width: {uploadPercent}%"></div></div>
      </div>
    {/if}

    <div class="card clipboard-box" transition:fly={{ y: 10, duration: 300, delay: 200 }}>
      <div class="card-header"><span>📋 کلیپ‌بورد اشتراکی</span></div>
      <textarea bind:value={clipboardText} placeholder="متن..."></textarea>
      <div class="clipboard-actions">
        <button class="btn-outline" onclick={clearClipboard} style="padding: 8px 16px;">🗑 پاک</button>
        <button class="btn-primary" onclick={sendClipboard} style="padding: 8px 16px;">🚀 ارسال</button>
      </div>
    </div>

    <div transition:fly={{ y: 10, duration: 300, delay: 250 }}>
      <div class="card-header" style="padding: 0 4px;">
        <span>📂 فایل‌ها</span>
        <button class="btn-refresh" onclick={loadFiles} title="Refresh">↻</button>
      </div>
      
      <div class="file-list">
        {#if files.length === 0}
          <div class="empty-state" transition:fade>
            <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" stroke-linejoin="round">
              <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path>
              <line x1="9" y1="14" x2="15" y2="14"></line>
            </svg>
            <h3>{t.emptyTitle}</h3>
            <p>{t.emptySub}</p>
          </div>
        {:else}
          {#each files as f (f.name)}
            <div class="file-item" animate:flip={{ duration: 200 }} in:fly={{ y: 20, duration: 300 }} out:fade={{ duration: 200 }}>
              <div class="file-icon">{f.is_dir ? '📁' : '📄'}</div>
              <div class="file-info"><div class="file-name">{f.name}</div><div class="file-meta">{f.is_dir ? 'Folder' : formatSize(f.size)} • {formatDate(f.modified)}</div></div>
              
              <div class="file-actions">
                {#if !f.is_dir}
                  <a href="/download/{encodeURIComponent(f.name)}" download class="tooltip-btn" data-tooltip={t.download}>⬇</a>
                {/if}
                <button class="del tooltip-btn" data-tooltip={t.delete} onclick={() => deleteFile(encodeURIComponent(f.name))}>🗑</button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </main>
</div>

{#if toastMsg}
  <div class="toast show {toastType === 'error' ? 'error' : ''}" transition:fly={{ y: 50, duration: 300 }}>
    {toastMsg}
  </div>
{/if}

<style>
  header { background: rgba(15, 23, 42, 0.8); backdrop-filter: blur(16px); border-bottom: 1px solid var(--border); padding: 12px 24px; position: sticky; top: 0; z-index: 50; display: flex; align-items: center; justify-content: space-between; }
  .header-left { display: flex; align-items: center; gap: 12px; }
  .logo-icon { width: 32px; height: 32px; border-radius: 8px; background: var(--gradient); display: flex; align-items: center; justify-content: center; font-size: 16px; color: #fff; }
  .brand h1 { margin: 0; font-size: 16px; font-weight: 600; }
  .brand .node-info { font-size: 11px; color: var(--primary-hi); display: flex; align-items: center; gap: 4px; }
  .node-dot { width: 6px; height: 6px; background: var(--primary); border-radius: 50%; display: inline-block; }
  .header-actions { display: flex; gap: 8px; }
  .icon-btn { background: transparent; border: 1px solid transparent; color: var(--text-muted); width: 36px; height: 36px; border-radius: 10px; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all .2s; }
  .icon-btn:hover { background: var(--surface-hover); color: var(--text); }
  .icon-btn.text { width: auto; padding: 0 12px; font-size: 12px; font-weight: 700; }
  
  .layout { display: flex; flex: 1; max-width: 1440px; margin: 0 auto; width: 100%; padding: 24px; gap: 24px; }
  .sidebar { width: 320px; flex-shrink: 0; display: flex; flex-direction: column; gap: 20px; }
  .card { background: var(--surface); border: 1px solid var(--border); border-radius: 16px; padding: 20px; box-shadow: var(--shadow); transition: border-color .2s; }
  .card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; font-size: 14px; font-weight: 600; }
  
  .qr-container { background: #fff; border-radius: 12px; padding: 16px; display: flex; align-items: center; justify-content: center; aspect-ratio: 1/1; max-width: 200px; margin: 0 auto 16px; }
  .qr-container img { width: 100%; height: 100%; object-fit: contain; }
  .pin-card { background: linear-gradient(180deg, var(--surface) 0%, rgba(15, 23, 42, 0.4) 100%); border: 1px solid var(--border-hi); }
  .pin-display { background: rgba(0, 0, 0, 0.3); border-radius: 12px; padding: 16px; text-align: center; margin-bottom: 12px; border: 1px inset rgba(255,255,255,0.05); }
  .pin-digits { font-size: 36px; font-weight: 700; letter-spacing: 6px; color: var(--primary-hi); font-family: 'JetBrains Mono', monospace; direction: ltr; display: inline-block; }
  .pin-hint { font-size: 12px; color: var(--text-muted); text-align: center; }
  .pin-actions { display: flex; gap: 8px; margin-top: 12px; }
  
  .btn-outline { flex: 1; background: var(--surface-hover); border: 1px solid var(--border); color: var(--text); padding: 10px; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer; transition: all .2s; display: flex; align-items: center; justify-content: center; gap: 6px; font-family: inherit; }
  .btn-outline:hover { border-color: var(--border-hi); background: rgba(255,255,255,0.05); }
  
  .main-content { flex: 1; display: flex; flex-direction: column; gap: 24px; min-width: 0; }
  .dropzone { border: 2px dashed var(--border); border-radius: 20px; padding: 40px 20px; text-align: center; cursor: pointer; transition: all .2s; background: rgba(17, 24, 39, 0.4); display: flex; flex-direction: column; align-items: center; gap: 16px; position: relative; overflow: hidden; }
  .dropzone:hover, .dropzone.drag { border-color: var(--primary); background: rgba(14, 165, 233, 0.05); box-shadow: var(--glow); }
  .dropzone:focus-visible { outline: none; border-color: var(--primary); box-shadow: var(--glow); }
  .dropzone-icon { width: 64px; height: 64px; border-radius: 16px; background: var(--gradient); color: #fff; display: flex; align-items: center; justify-content: center; font-size: 28px; box-shadow: var(--glow); }
  .dropzone h2 { margin: 0; font-size: 20px; font-weight: 600; }
  
  .btn-primary { background: var(--gradient); color: #fff; border: none; padding: 12px 24px; border-radius: 10px; font-size: 14px; font-weight: 600; cursor: pointer; transition: all .2s; font-family: inherit; display: inline-flex; align-items: center; justify-content: center; gap: 8px; }
  .btn-primary:hover { filter: brightness(1.1); box-shadow: var(--glow); }
  
  .clipboard-box textarea { width: 100%; background: rgba(0, 0, 0, 0.2); border: 1px solid var(--border); border-radius: 10px; padding: 12px; color: var(--text); font-family: 'JetBrains Mono', monospace; font-size: 13px; resize: vertical; min-height: 80px; margin-bottom: 12px; transition: border-color .2s; direction: ltr; }
  .clipboard-box textarea:focus { outline: none; border-color: var(--primary); }
  .clipboard-actions { display: flex; justify-content: flex-end; gap: 8px; }
  
  .file-list { display: flex; flex-direction: column; gap: 8px; }
  .file-item { display: flex; align-items: center; gap: 12px; padding: 12px 16px; background: var(--surface); border: 1px solid var(--border); border-radius: 12px; transition: border-color .2s, background .2s; }
  .file-item:hover { border-color: var(--border-hi); background: var(--surface-hover); }
  .file-icon { width: 40px; height: 40px; border-radius: 10px; background: var(--surface-hover); border: 1px solid var(--border); display: flex; align-items: center; justify-content: center; font-size: 18px; color: var(--primary-hi); flex-shrink: 0; }
  .file-info { flex: 1; min-width: 0; }
  .file-name { font-size: 14px; font-weight: 500; margin-bottom: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .file-meta { display: flex; gap: 8px; font-size: 11px; color: var(--text-muted); font-family: 'JetBrains Mono', monospace; }
  .file-actions { display: flex; gap: 8px; }

  /* Compact Refresh Button */
  .btn-refresh {
    background: var(--surface-hover);
    border: 1px solid var(--border);
    color: var(--text);
    width: 28px; height: 28px;
    border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.3s ease;
    font-family: inherit;
  }
  .btn-refresh:hover {
    background: var(--border-hi);
    transform: rotate(180deg);
  }

  /* Compact Empty State Styles */
  .empty-state {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    padding: 24px 16px; /* Reduced from 40px */
    text-align: center; color: var(--text-muted);
    background: var(--surface); border: 1px dashed var(--border); border-radius: 12px;
  }
  .empty-state svg {
    margin-bottom: 12px;
    color: var(--text-muted);
    opacity: 0.5;
  }
  .empty-state h3 {
    margin: 0 0 4px;
    font-size: 14px; /* Reduced from 16px */
    font-weight: 600;
    color: var(--text);
  }
  .empty-state p {
    margin: 0;
    font-size: 12px; /* Reduced from 13px */
  }

  /* Custom CSS Tooltips */
  .tooltip-btn {
    position: relative;
    width: 32px; height: 32px; border-radius: 8px; display: flex; align-items: center; justify-content: center;
    background: var(--surface-hover); border: 1px solid var(--border); color: var(--text);
    cursor: pointer; transition: all .2s; text-decoration: none;
  }
  .tooltip-btn:hover { background: var(--border-hi); }
  .tooltip-btn.del:hover { background: var(--danger); border-color: var(--danger); color: #fff; }
  
  .tooltip-btn::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: 120%;
    left: 50%;
    transform: translateX(-50%) translateY(10px);
    background: var(--surface-hover);
    color: var(--text);
    padding: 6px 12px;
    border-radius: 6px;
    font-size: 11px;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.2s, transform 0.2s;
    box-shadow: 0 4px 12px rgba(0,0,0,0.5);
    border: 1px solid var(--border-hi);
    z-index: 10;
    font-family: 'Geist', 'Vazirmatn', sans-serif;
    font-weight: 500;
  }
  .tooltip-btn::before {
    content: '';
    position: absolute;
    bottom: 100%;
    left: 50%;
    transform: translateX(-50%) translateY(10px);
    border-width: 5px;
    border-style: solid;
    border-color: var(--border-hi) transparent transparent transparent;
    pointer-events: none;
    opacity: 0;
    transition: opacity 0.2s, transform 0.2s;
    z-index: 10;
  }
  .tooltip-btn:hover::after,
  .tooltip-btn:hover::before {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }

  .progress-card { padding: 16px; }
  .progress-header { display: flex; justify-content: space-between; font-size: 13px; margin-bottom: 8px; }
  .progress-bar { height: 6px; background: var(--border); border-radius: 4px; overflow: hidden; }
  .progress-fill { height: 100%; background: var(--gradient); width: 0%; transition: width .2s; }
  
  .login-overlay { position: fixed; inset: 0; background: rgba(11, 15, 25, 0.95); backdrop-filter: blur(10px); z-index: 999; display: flex; align-items: center; justify-content: center; padding: 20px; }
  .login-card { background: var(--surface); border: 1px solid var(--border); border-radius: 24px; padding: 40px 32px; width: 100%; max-width: 400px; text-align: center; box-shadow: var(--shadow); }
  .pin-input { width: 100%; background: rgba(0,0,0,0.3); border: 2px solid var(--border); border-radius: 12px; padding: 16px; font-size: 32px; text-align: center; color: var(--text); font-family: 'JetBrains Mono', monospace; letter-spacing: 8px; outline: none; margin-bottom: 16px; direction: ltr; }
  .pin-input:focus { border-color: var(--primary); box-shadow: var(--glow); }
  
  .toast { position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%); background: var(--surface); border: 1px solid var(--border); color: var(--text); padding: 12px 24px; border-radius: 100px; font-size: 13px; font-weight: 500; box-shadow: var(--shadow); z-index: 1000; pointer-events: none; }
  .toast.error { border-color: var(--danger); background: var(--danger-bg); }
  
  @media (max-width: 900px) { .layout { flex-direction: column; padding: 16px; } .sidebar { width: 100%; display: grid; grid-template-columns: 1fr 1fr; gap: 16px; } .qr-container { max-width: 140px; } .pin-digits { font-size: 28px; } }
  @media (max-width: 600px) { .sidebar { grid-template-columns: 1fr; } .dropzone { padding: 24px 16px; } .dropzone h2 { font-size: 16px; } .file-item { padding: 10px 12px; } }
</style>