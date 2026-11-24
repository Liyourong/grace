const el = id => document.getElementById(id);
const panel = document.createElement("section");
panel.id = "logs-view";
panel.className = "view";
panel.hidden = true;
panel.innerHTML = `
  <div class="panehead"><h1>Request logs</h1><span>Last 7 days</span></div>
  <div class="log-filters">
    <label>Pod <select id="logPod"><option value="">All pods</option></select></label>
    <label>Status <input id="logStatus" type="search" placeholder="e.g. 401" maxlength="3"></label>
    <label>Trace ID <input id="logId" type="search" placeholder="Filter by ID" maxlength="32"></label>
  </div>
  <p id="logMessage" role="status"></p>
  <div class="log-scroll"><table><thead><tr><th>Completed</th><th>Operation</th><th>Pod</th><th>Status</th><th>Duration</th></tr></thead><tbody id="logRows"></tbody></table></div>
  <div class="log-pages"><button id="newer">Newer</button><button id="older">Older</button></div>
  <section class="trace-detail" aria-label="Selected trace"><h2>Trace</h2><div id="traceDetail">Select a request.</div></section>`;
document.querySelector(".center").append(panel);
const link = document.createElement("a");
link.href = "#logs-view";
link.textContent = "Logs";
document.querySelector(".toolbar nav").append(link);

let rows = [], selected = "", timer, controller, nextCursor = null;
const pages = [];
const duration = us => (us / 1000).toLocaleString(undefined, {maximumFractionDigits: 3}) + " ms";
const active = () => !document.hidden && location.hash === link.hash;
function drawRows() {
  const content = document.createDocumentFragment();
  for (const r of rows) {
    const row = document.createElement("tr");
    row.classList.toggle("selected", r.id === selected);
    row.insertCell().textContent = new Date(r.time).toLocaleString();
    const button = document.createElement("button");
    button.textContent = r.operation;
    button.title = r.id;
    button.onclick = () => { activity("view_trace"); openTrace(r); };
    row.insertCell().append(button);
    for (const value of [r.pod, r.status, duration(r.us)]) row.insertCell().textContent = value;
    content.append(row);
  }
  el("logRows").replaceChildren(content);
  el("logMessage").textContent = rows.length + " requests on this page";
  el("newer").disabled = !pages.length;
  el("older").disabled = !nextCursor;
}
function openTrace(t) {
    selected = t.id;
    drawRows();
    const detail = el("traceDetail");
    detail.textContent = t.operation + " · " + t.method + " " + t.route + " · " + t.pod + " · " + t.status + " · " + duration(t.us) + " · Trace " + t.id;
    if (t.group && t.group !== t.id) detail.append(" · Operation ID " + t.group);
    const list = document.createElement("ol");
    for (const s of t.steps) {
      const li = document.createElement("li");
      li.textContent = s.name + " · " + duration(s.us) + (s.ok ? "" : " · Failed");
      if (!s.ok) li.className = "failed";
      list.append(li);
    }
    if (t.status >= 400 && !t.steps.some(s => !s.ok)) {
      const li = document.createElement("li");
      li.textContent = t.rejected_at ? t.rejected_at + " · Failed" : "Request failed.";
      li.className = "failed";
      list.append(li);
    }
    detail.append(list);
}
async function refresh(signal) {
  try {
    const params = new URLSearchParams({pod:el("logPod").value, status:el("logStatus").value.trim(), trace:el("logId").value.trim()});
    if (pages.length) params.set("before", pages.at(-1));
    const res = await operatorFetch("/api/devops/logs?" + params, {signal, cache:"no-store"});
    if (!res.ok) throw Error("Logs unavailable (" + res.status + ").");
    const data = await res.json();
    if (signal.aborted) return;
    nextCursor = data.before;
    rows = data.items;
      const pod = el("logPod"), value = pod.value;
      const names = new Set(data.pods);
      if (value) names.add(value);
      pod.replaceChildren(new Option("All pods", ""), ...[...names].sort().map(n => new Option(n,n)));
      pod.value = value;
      drawRows();
  } catch (err) {
    if (signal.aborted) return;
    rows = []; selected = ""; nextCursor = null; el("older").disabled = true;
    el("logRows").replaceChildren();
    el("traceDetail").textContent = "Select a request.";
    el("logMessage").textContent = err.message;
    el("logMessage").dataset.failed = "1";
  }
  if (!signal.aborted && active()) timer = setTimeout(() => refresh(signal), 2000);
}
function toggle() {
  clearTimeout(timer);
  controller?.abort();

  if (active()) {
    controller = new AbortController();
    refresh(controller.signal);

  }
}
function resetPage() {
  selected = "";
  el("traceDetail").textContent = "Select a request.";
  toggle();
}
for (const id of ["logPod", "logStatus", "logId"]) el(id).oninput = () => {
  pages.length = 0;
  resetPage();
};
el("newer").onclick = () => { pages.pop(); activity("view_logs"); resetPage(); };
el("older").onclick = () => { if (nextCursor) pages.push(nextCursor); activity("view_logs"); resetPage(); };
window.addEventListener("hashchange", toggle);
document.addEventListener("visibilitychange", toggle);
toggle();

const contexts = { Minikube: "grace-dash-demo", DOKS: "do-tor1-grace" };
const env = el("commandEnv");
env.value = environment;

function showPersistenceSteps() {
  const target = env.value;
  const compose = target === "Compose";
  const kube = `kubectl --context ${contexts[target]} -n grace`;
  const exec = compose ? "docker compose exec -T db" : `${kube} exec db-0 --`;
  const read = `${exec} psql -X -U grace -d grace -c "
SELECT pg_postmaster_start_time() AS db_started, question_id, saved_answer
FROM answers JOIN users ON users.id = answers.user_id
WHERE username = 'grace_test'
ORDER BY answers.updated_at DESC, question_id LIMIT 1;"`;
  const restart = compose
    ? "docker compose up -d --no-deps --force-recreate --wait db"
    : `${kube} rollout restart statefulset/db &&
${kube} rollout status statefulset/db --timeout=180s`;
  el("commandHelp").textContent = "First, submit an answer as grace_test. " + (compose
    ? "Run each step in the active Compose project directory."
    : `Context: ${contexts[target]}. Step 2 briefly restarts the ${target} database.`);
  const steps = el("persistenceSteps");
  steps.replaceChildren();
  for (const [title, command] of [
    ["Read the saved answer", read],
    ["Restart PostgreSQL", restart],
  ]) {
    const item = document.createElement("li");
    const heading = document.createElement("strong");
    heading.textContent = title;
    const code = document.createElement("pre");
    code.textContent = command;
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = "Copy";
    button.setAttribute("aria-label", "Copy command: " + title);
    const status = document.createElement("span");
    status.setAttribute("role", "status");
    button.onclick = async () => {
      try {
        await navigator.clipboard.writeText(command);
        status.textContent = "Copied";
        activity("copy_" + target.toLowerCase());
      } catch {
        const range = document.createRange();
        range.selectNodeContents(code);
        const selection = window.getSelection();
        selection.removeAllRanges();
        selection.addRange(range);
        status.textContent = "Press Ctrl+C or ⌘C to copy.";
      }
    };
    item.append(heading, code, button, status);
    steps.append(item);
  }
}
el("persistence").hidden = false;
env.onchange = () => { showPersistenceSteps(); activity("select_environment"); };
showPersistenceSteps();
