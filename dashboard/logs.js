const el = id => document.getElementById(id);
const panel = document.createElement("section");
panel.id = "logs-view";
panel.className = "view";
panel.hidden = true;
panel.innerHTML = `
  <div class="panehead"><h1>Request logs</h1><span>Latest 500</span></div>
  <div class="log-filters">
    <label>Pod <select id="logPod"><option value="">All pods</option></select></label>
    <label>Status <input id="logStatus" type="search" placeholder="e.g. 401" maxlength="3"></label>
    <label>Trace ID <input id="logId" type="search" placeholder="Filter by ID" maxlength="32"></label>
  </div>
  <p id="logMessage" role="status"></p>
  <div class="log-scroll"><table><thead><tr><th>Completed</th><th>Request</th><th>Pod</th><th>Status</th><th>Duration</th></tr></thead><tbody id="logRows"></tbody></table></div>
  <section class="trace-detail" aria-label="Selected trace"><h2>Trace</h2><div id="traceDetail">Select a request.</div></section>`;
document.querySelector(".center").append(panel);
const link = document.createElement("a");
link.href = "#logs-view";
link.textContent = "Logs";
document.querySelector(".toolbar nav").append(link);

let rows = [], selected = "", timer, controller, selectionRequest;
const duration = us => (us / 1000).toLocaleString(undefined, {maximumFractionDigits: 3}) + " ms";
const active = () => !document.hidden && location.hash === link.hash;

async function get(path, signal) {
  const key = localStorage.getItem("grace.presenter");
  if (!key) throw Error("Open your private presenter link to unlock logs.");
  const res = await fetch(path, {headers: {Authorization: "Bearer " + key}, cache: "no-store", signal});
  if (res.status === 401 || res.status === 403) throw Error("Open your private presenter link to unlock logs.");
  if (res.status === 404) throw Error("This trace has expired.");
  if (!res.ok) throw Error("Logs unavailable (" + res.status + ").");
  return res.json();
}
function drawRows() {
  const filtered = rows.filter(r => (!el("logPod").value || el("logPod").value === r.pod)
    && String(r.status).startsWith(el("logStatus").value.trim())
    && r.id.includes(el("logId").value.trim().toLowerCase()));
  const content = document.createDocumentFragment();
  for (const r of filtered) {
    const row = document.createElement("tr");
    row.classList.toggle("selected", r.id === selected);
    row.insertCell().textContent = new Date(r.time).toLocaleTimeString();
    const button = document.createElement("button");
    button.textContent = r.method + " " + r.route;
    button.title = r.id;
    button.onclick = () => openTrace(r.id);
    row.insertCell().append(button);
    for (const value of [r.pod, r.status, duration(r.us)]) row.insertCell().textContent = value;
    content.append(row);
  }
  el("logRows").replaceChildren(content);
  el("logMessage").textContent = filtered.length + " of " + rows.length + " requests";
}
async function openTrace(id) {
  selectionRequest?.abort();
  selectionRequest = new AbortController();
  selected = id;
  drawRows();
  const detail = el("traceDetail");
  detail.textContent = "Loading trace…";
  try {
    const t = await get("/api/traces/" + id, selectionRequest.signal);
    if (selected !== id || !active()) return;
    detail.textContent = t.id + " · " + t.pod + " · HTTP " + t.status + " · " + duration(t.us);
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
  } catch (err) {
    if (err.name !== "AbortError" && selected === id) detail.textContent = err.message;
  }
}
async function refresh(signal) {
  try {
    const next = await get("/api/logs", signal);
    if (signal.aborted) return;
    const changed = rows.length !== next.length || next.some((r,i) => rows[i]?.id !== r.id);
    if (changed || !el("logRows").children.length || el("logMessage").dataset.failed) {
      delete el("logMessage").dataset.failed;
      rows = next;
      const pod = el("logPod"), value = pod.value;
      const names = new Set(rows.map(r => r.pod));
      if (value) names.add(value);
      pod.replaceChildren(new Option("All pods", ""), ...[...names].sort().map(n => new Option(n,n)));
      pod.value = value;
      drawRows();
    }
  } catch (err) {
    if (signal.aborted) return;
    rows = []; selected = ""; selectionRequest?.abort();
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
  selectionRequest?.abort();
  if (active()) {
    controller = new AbortController();
    refresh(controller.signal);
    if (selected) openTrace(selected);
  }
}
for (const id of ["logPod", "logStatus", "logId"]) el(id).addEventListener("input",drawRows);
window.addEventListener("hashchange",toggle);
document.addEventListener("visibilitychange",toggle);
window.dispatchEvent(new Event("hashchange"));
