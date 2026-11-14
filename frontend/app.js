'use strict';

const app = document.getElementById('app');

// All user content goes through text nodes, never innerHTML.
function h(tag, props = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(props)) {
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'class') el.className = v;
    else if (v === true) el.setAttribute(k, '');
    else if (v !== false && v != null) el.setAttribute(k, v);
  }
  for (const c of children.flat()) if (c != null && c !== false) el.append(c instanceof Node ? c : String(c));
  return el;
}

// Returns {ok, status, data} or {ok:false, status, error, uncertain}. Never retries: a replayed upload creates a second paper.
async function api(method, path, body) {
  const opts = { method, credentials: 'same-origin', headers: {} };
  if (body !== undefined) {
    opts.headers['Content-Type'] = 'application/json';
    opts.body = typeof body === 'string' ? body : JSON.stringify(body);
  }
  let res;
  try {
    res = await fetch(path, opts);
  } catch {
    return { ok: false, status: 0, uncertain: true, error: { code: 'network', message: '网络连接失败' } };
  }
  let data = null;
  try { data = await res.json(); } catch { /* 204 or a non-JSON proxy page */ }
  if (res.ok) return { ok: true, status: res.status, data };
  const error = data?.error ?? { code: `http_${res.status}`, message: `请求失败（HTTP ${res.status}）` };
  return { ok: false, status: res.status, error, uncertain: res.status >= 500 };
}

function notice(kind, message, ...extra) {
  return h('div', { class: `notice ${kind}`, role: kind === 'error' ? 'alert' : 'status' }, h('div', {}, message), ...extra);
}

function errorBox(error, ...extra) {
  return notice('error', error.message,
    error.details?.length ? h('ul', {}, error.details.map((d) => h('li', {}, d))) : null, ...extra);
}

const fmtTime = (iso) => new Date(iso).toLocaleString();
// Same-title papers are distinct uploads; the upload time tells them apart in the sidebar.
const shortTime = (iso) => new Date(iso).toLocaleString([], { month: 'numeric', day: 'numeric', hour: '2-digit', minute: '2-digit' });
const pct = (s) => (s.total ? Math.round((s.answered / s.total) * 100) : 0);

const THEMES = { system: ['◐', '跟随系统'], light: ['☀', '浅色'], dark: ['☾', '深色'] };
const NEXT_THEME = { system: 'light', light: 'dark', dark: 'system' };

function themeButton(extraClass = '') {
  const btn = h('button', { class: `btn ghost small theme-btn ${extraClass}` });
  const paint = () => {
    const [icon, label] = THEMES[getTheme()] ?? THEMES.system;
    btn.textContent = icon;
    btn.title = `主题：${label}（点击切换）`;
    btn.setAttribute('aria-label', btn.title);
  };
  btn.addEventListener('click', () => { setTheme(NEXT_THEME[getTheme()] ?? 'light'); paint(); });
  paint();
  return btn;
}

// ---------- boot ----------

async function boot() {
  app.replaceChildren(h('div', { class: 'center muted' }, '加载中…'));
  const r = await api('GET', '/api/auth/me');
  if (r.ok) return enterApp(r.data.user);
  if (r.status === 401) return showAuth('login');
  app.replaceChildren(h('div', { class: 'center' }, errorBox(r.error, h('button', { class: 'btn', onclick: boot }, '重试'))));
}

// ---------- auth ----------

function showAuth(mode, message) {
  const isLogin = mode === 'login';
  const msg = h('div', {}, message ? notice('warn', message) : null);
  const username = h('input', { class: 'field', type: 'text', name: 'username', placeholder: '用户名', 'aria-label': '用户名',
    autocomplete: 'username' });
  const password = h('input', { class: 'field', type: 'password', name: 'password', placeholder: '密码', 'aria-label': '密码',
    autocomplete: isLogin ? 'current-password' : 'new-password' });
  const submit = h('button', { class: 'btn primary wide', type: 'submit' }, isLogin ? '登录' : '创建账户');

  const form = h('form', { class: 'auth', onsubmit: async (e) => {
    e.preventDefault();
    submit.disabled = true;
    submit.textContent = '请稍候…';
    const r = await api('POST', isLogin ? '/api/auth/login' : '/api/auth/register',
      { username: username.value, password: password.value });
    submit.disabled = false;
    submit.textContent = isLogin ? '登录' : '创建账户';
    if (r.ok) return enterApp(r.data.user);
    msg.replaceChildren(!isLogin && r.uncertain
      ? errorBox({ message: '暂时无法确认账户是否已创建。请先尝试用这组用户名和密码登录。' })
      : errorBox(r.error));
  } },
    h('div', { class: 'logo', 'aria-hidden': 'true' }, 'G'),
    h('h1', {}, isLogin ? '欢迎回来' : '创建你的账户'),
    h('p', { class: 'muted' }, isLogin ? '登录后继续你的练习' : '用自己的试卷练习语言'),
    username,
    password,
    msg,
    submit,
    h('p', { class: 'muted small' }, isLogin ? '还没有账户？' : '已有账户？',
      h('button', { type: 'button', class: 'link', onclick: () => showAuth(isLogin ? 'register' : 'login') },
        isLogin ? '注册' : '登录')));
  app.replaceChildren(h('div', { class: 'center' }, form), themeButton('corner'));
  username.focus();
}

// ---------- app shell: sidebar + main ----------

let shell = null; // { list, main, activeId }

function enterApp(user) {
  const list = h('nav', { class: 'paper-list', 'aria-label': '我的试卷' });
  const main = h('main', { class: 'main' });
  shell = { list, main, activeId: null };
  const sidebar = h('aside', { class: 'sidebar' },
    h('div', { class: 'brand' }, 'Grace'),
    h('button', { class: 'new', onclick: openHome }, h('span', { 'aria-hidden': 'true' }, '＋'), ' 上传新试卷'),
    h('div', { class: 'section-label' }, '我的试卷'),
    list,
    h('div', { class: 'account' },
      h('span', { class: 'avatar', 'aria-hidden': 'true' }, user.username[0].toUpperCase()),
      h('span', { class: 'name' }, user.username),
      themeButton(),
      h('button', { class: 'btn ghost small', onclick: logout }, '退出')));
  app.replaceChildren(h('div', { class: 'shell' }, sidebar, main));
  loadList();
  openHome();
}

async function logout() {
  const r = await api('POST', '/api/auth/logout');
  if (!r.ok && r.status !== 401) return alert(`退出失败，你仍处于登录状态：${r.error.message}`);
  shell = null;
  showAuth('login');
}

function expired(what) { shell = null; showAuth('login', `登录已失效，${what}。请重新登录。`); }

async function loadList() {
  const { list } = shell;
  if (!list.childElementCount) list.replaceChildren(h('div', { class: 'muted small pad' }, '加载中…'));
  const r = await api('GET', '/api/papers');
  if (!shell || shell.list !== list) return;
  if (!r.ok) {
    if (r.status === 401) return expired('无法读取试卷列表');
    return list.replaceChildren(h('div', { class: 'small pad bad' }, '列表读取失败，不代表没有试卷。',
      h('button', { class: 'link', onclick: loadList }, '重试')));
  }
  const papers = r.data.papers;
  if (!papers.length) return list.replaceChildren(h('div', { class: 'muted small pad' }, '还没有试卷'));
  list.replaceChildren(...papers.map((p) => h('button', {
    class: `paper-item${p.id === shell.activeId ? ' active' : ''}`,
    'data-id': p.id,
    title: `上传于 ${fmtTime(p.uploaded_at)}${p.last_activity_at ? ` · 最后练习 ${fmtTime(p.last_activity_at)}` : ''}`,
    onclick: () => openPaper(p.id),
  }, h('span', { class: 'text' }, h('span', { class: 'title' }, p.title), h('span', { class: 'date' }, shortTime(p.uploaded_at))),
    h('span', { class: 'meta' }, `${p.stats.answered}/${p.stats.total}`))));
}

function setActive(id) {
  shell.activeId = id;
  for (const el of shell.list.querySelectorAll('.paper-item')) el.classList.toggle('active', el.dataset.id === id);
}

// ---------- home: upload ----------

function openHome() {
  setActive(null);
  shell.main.replaceChildren(h('div', { class: 'column home' },
    h('h1', { class: 'hero' }, '今天想练点什么？'),
    h('p', { class: 'muted' }, '上传一份你在其他工具里做好的 JSON 试卷，Grace 会保存到云端，换设备登录也能继续练。'),
    uploadCard(),
    h('details', { class: 'format' },
      h('summary', {}, '试卷文件格式'),
      h('p', { class: 'small muted' }, '根对象包含 format_version（1）、title、可选 description、questions 数组。题型 choice 需要 options 与 correct_option_id；translation / sentence 需要 accepted_answers。每题都需要 id、prompt、correct_answer、explanation。'),
      h('pre', {}, '{\n  "format_version": 1,\n  "title": "法语基础练习",\n  "questions": [\n    { "id": "q1", "type": "translation",\n      "prompt": "将“谢谢”翻译为法语。",\n      "accepted_answers": ["Merci"],\n      "correct_answer": "Merci.",\n      "explanation": "常用感谢语。" }\n  ]\n}'))));
}

function uploadCard() {
  const input = h('input', { type: 'file', accept: '.json,application/json', class: 'visually-hidden' });
  const fileRow = h('div');
  const status = h('div');
  let file = null;

  const choose = (f) => {
    file = f ?? null;
    status.replaceChildren();
    fileRow.replaceChildren(file ? h('div', { class: 'file-row' },
      h('span', { class: 'file-icon', 'aria-hidden': 'true' }, '{ }'),
      h('span', { class: 'file-name' }, file.name, h('span', { class: 'muted small' }, ` · ${(file.size / 1024).toFixed(1)} KB · 尚未上传`)),
      h('button', { class: 'btn primary', onclick: upload }, '上传')) : null);
  };

  async function upload(e) {
    const btn = e.currentTarget;
    btn.disabled = true;
    btn.textContent = '读取中…';
    let text;
    try {
      text = await file.text();
    } catch {
      btn.disabled = false;
      btn.textContent = '上传';
      return status.replaceChildren(errorBox({ message: '无法读取本地文件，请重新选择。' }));
    }
    btn.textContent = '保存中…';
    status.replaceChildren(h('p', { class: 'muted small' }, '正在上传并保存到云端，请勿关闭页面。'));
    const r = await api('POST', '/api/papers', text);
    btn.disabled = false;
    btn.textContent = '上传';

    if (r.ok) {
      const p = r.data.paper;
      input.value = '';
      choose(null);
      status.replaceChildren(notice('success', `已保存到云端：《${p.title}》，共 ${p.question_count} 题`,
        h('button', { class: 'btn primary', onclick: () => openPaper(p.id) }, '开始练习')));
      loadList();
    } else if (r.status === 401) {
      expired('试卷未上传');
    } else if (r.uncertain) {
      status.replaceChildren(errorBox(
        { message: '暂时无法确认是否保存成功。请先看左侧列表核对；如果再次上传，会创建另一份独立的新试卷。' },
        h('button', { class: 'btn', onclick: loadList }, '刷新列表')));
    } else {
      status.replaceChildren(errorBox({ ...r.error, message: `未导入：${r.error.message}` }));
    }
  }

  input.addEventListener('change', () => choose(input.files[0]));
  const zone = h('label', { class: 'dropzone' },
    input,
    h('span', { class: 'drop-icon', 'aria-hidden': 'true' }, '↑'),
    h('span', {}, '点击选择或拖入 .json 文件'),
    h('span', { class: 'muted small' }, '每次上传都会创建一份新试卷，最大 1 MiB'));
  zone.addEventListener('dragover', (e) => { e.preventDefault(); zone.classList.add('over'); });
  zone.addEventListener('dragleave', () => zone.classList.remove('over'));
  zone.addEventListener('drop', (e) => { e.preventDefault(); zone.classList.remove('over'); choose(e.dataTransfer.files[0]); });

  return h('div', { class: 'upload' }, zone, fileRow, status);
}

// ---------- practice ----------

async function openPaper(paperId) {
  setActive(paperId);
  const main = shell.main;
  main.replaceChildren(h('div', { class: 'column muted' }, '加载试卷中…'));
  const r = await api('GET', `/api/papers/${encodeURIComponent(paperId)}`);
  if (shell?.main !== main || shell.activeId !== paperId) return;
  if (!r.ok) {
    if (r.status === 401) return expired('无法打开试卷');
    return main.replaceChildren(h('div', { class: 'column' }, errorBox(r.error,
      h('button', { class: 'btn', onclick: () => openPaper(paperId) }, '重试'))));
  }

  const paper = r.data;
  if (!paper.questions.length) return main.replaceChildren(h('div', { class: 'column muted' }, '这份试卷没有题目。'));
  const drafts = {}; // unsubmitted text lives only in this page; it is never saved or restored
  let index = Math.max(0, paper.questions.findIndex((q) => q.id === paper.current_question_id));
  const base = `/api/papers/${encodeURIComponent(paperId)}`;
  // Answer and position writes both move the saved position, so send them strictly in click order.
  let queue = Promise.resolve();
  const send = (method, path, body) => (queue = queue.then(() => api(method, path, body)));

  const bar = h('div', { class: 'bar' }, h('div'));
  const statsLine = h('div', { class: 'muted small' });
  const dots = h('div', { class: 'dots' });
  const warn = h('div');
  const body = h('div', { class: 'qcard' });

  function renderStats() {
    const s = paper.stats;
    bar.firstChild.style.width = `${pct(s)}%`;
    statsLine.textContent = `已答 ${s.answered} / ${s.total} · 答对 ${s.correct} · 答错 ${s.incorrect}`;
  }

  function renderDots() {
    dots.replaceChildren(...paper.questions.map((q, i) => h('button', {
      class: ['dot', q.answer ? (q.answer.is_correct ? 'correct' : 'incorrect') : '', i === index ? 'current' : ''].join(' '),
      'aria-label': `第 ${i + 1} 题${q.answer ? (q.answer.is_correct ? '，答对' : '，答错') : '，未作答'}`,
      'aria-current': i === index ? 'step' : false,
      onclick: () => go(i),
    }, String(i + 1))));
  }

  async function go(i) {
    index = i;
    render();
    const r = await send('PUT', `${base}/position`, { question_id: paper.questions[i].id });
    if (r.ok) warn.replaceChildren();
    else if (r.status === 401) expired('当前位置未保存');
    else warn.replaceChildren(notice('warn', `当前位置未保存：${r.error.message}`));
  }

  async function submit(q, payload, controls) {
    controls.forEach((c) => { c.disabled = true; });
    const r = await send('PUT', `${base}/answers/${encodeURIComponent(q.id)}`, payload);
    controls.forEach((c) => { c.disabled = false; });
    const num = paper.questions.indexOf(q) + 1;
    if (r.ok) {
      q.answer = r.data.answer;
      paper.stats = r.data.stats;
      delete drafts[q.id];
      warn.replaceChildren();
      render();
      loadList();
    } else if (r.status === 401) {
      expired(`第 ${num} 题的答案未保存`);
    } else if (r.uncertain) {
      warn.replaceChildren(errorBox({ message: `无法确认第 ${num} 题的答案是否已保存，请重新打开试卷核对。` }));
    } else {
      warn.replaceChildren(errorBox({ message: `第 ${num} 题答案未保存：${r.error.message}` }));
    }
  }

  function feedback(q) {
    if (!q.answer) return null;
    const a = q.answer;
    const yours = q.kind === 'choice' ? (q.options.find((o) => o.id === a.saved_answer)?.text ?? a.saved_answer) : a.saved_answer;
    return h('div', { class: `feedback ${a.is_correct ? 'ok' : 'bad'}` },
      h('div', { class: 'verdict' }, a.is_correct ? '✓ 回答正确' : '✗ 回答错误'),
      q.kind !== 'choice' && h('div', {}, h('span', { class: 'muted' }, '你的答案　'), yours),
      h('div', {}, h('span', { class: 'muted' }, '参考答案　'), a.correct_answer),
      h('div', {}, h('span', { class: 'muted' }, '解析　　　'), a.explanation));
  }

  function renderQuestion() {
    const q = paper.questions[index];
    const kindLabel = { choice: '选择题', translation: '翻译题', sentence: '造句题' }[q.kind];
    let input;
    if (q.kind === 'choice') {
      const buttons = q.options.map((o, i) => {
        const chosen = q.answer?.saved_answer === o.id;
        return h('button', {
          class: `option${chosen ? (q.answer.is_correct ? ' chosen ok' : ' chosen bad') : ''}`,
          'aria-pressed': chosen ? 'true' : 'false',
          onclick: () => submit(q, { option_id: o.id }, buttons),
        }, h('span', { class: 'key', 'aria-hidden': 'true' }, String.fromCharCode(65 + i)), h('span', {}, o.text));
      });
      input = h('div', { class: 'options' }, buttons);
    } else {
      const area = h('textarea', { 'aria-label': '你的答案', placeholder: '输入你的答案，回车提交（Shift+回车换行）', rows: 2, maxlength: 2000 });
      area.value = drafts[q.id] ?? q.answer?.saved_answer ?? '';
      const btn = h('button', { class: 'send', 'aria-label': '提交答案', title: '提交' }, '↑');
      const submitText = () => {
        if (!area.value.trim()) return warn.replaceChildren(notice('warn', '请先输入答案。'));
        submit(q, { text: area.value }, [btn, area]);
      };
      const pending = h('div', { class: 'muted small' });
      const markDraft = () => {
        pending.textContent = q.answer && area.value !== q.answer.saved_answer ? '有未提交的修改，离开后不会保存' : '';
      };
      area.addEventListener('input', () => { drafts[q.id] = area.value; markDraft(); });
      // isComposing: Enter that confirms an IME candidate (e.g. pinyin) must not submit.
      area.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); submitText(); } });
      btn.addEventListener('click', submitText);
      markDraft();
      input = h('div', {}, h('div', { class: 'composer' }, area, btn), pending);
    }
    const last = index === paper.questions.length - 1;
    body.replaceChildren(
      h('div', { class: 'qhead' }, h('span', { class: 'chip' }, kindLabel), h('span', { class: 'muted small' }, `第 ${index + 1} / ${paper.questions.length} 题`)),
      h('p', { class: 'prompt' }, q.prompt),
      input,
      feedback(q),
      h('div', { class: 'qnav' },
        h('button', { class: 'btn ghost', disabled: index === 0, onclick: () => go(index - 1) }, '← 上一题'),
        last ? h('button', { class: 'btn primary', onclick: finish }, '结束练习')
          : h('button', { class: 'btn ghost', onclick: () => go(index + 1) }, '下一题 →')));
  }

  async function finish() {
    const r = await send('POST', `${base}/finish`);
    if (r.status === 401) return expired('未能结束练习');
    if (!r.ok) return warn.replaceChildren(errorBox(r.error));
    paper.stats = r.data.stats;
    const s = paper.stats;
    renderStats();
    body.replaceChildren(
      h('h2', {}, '练习完成'),
      h('div', { class: 'score' },
        h('div', {}, h('strong', { class: 'ok' }, s.correct), h('span', {}, '答对')),
        h('div', {}, h('strong', { class: 'bad' }, s.incorrect), h('span', {}, '答错')),
        h('div', {}, h('strong', {}, s.unanswered), h('span', {}, '未答'))),
      h('p', { class: 'muted small' }, '答案已保存，随时可以回来修改。'),
      h('div', { class: 'qnav' },
        h('button', { class: 'btn ghost', onclick: render }, '返回修改'),
        h('button', { class: 'btn primary', onclick: openHome }, '完成')));
  }

  function render() { renderStats(); renderDots(); renderQuestion(); }

  main.replaceChildren(h('div', { class: 'column' },
    h('h1', { class: 'paper-title' }, paper.title),
    paper.description && h('p', { class: 'muted' }, paper.description),
    h('div', { class: 'progress' }, bar, statsLine),
    dots, warn, body));
  render();
}

boot();
