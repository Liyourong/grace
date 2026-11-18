"use strict";
const audience = document.documentElement.hasAttribute("data-viewer");
if (!audience && location.origin === "http://127.0.0.1:19090" && /^#presenter=[a-f0-9]{64}$/.test(location.hash)) {
  localStorage.setItem("grace.presenter", location.hash.slice("#presenter=".length));
  history.replaceState(null, "", "/#topology-view");
}
const $ = (id) => document.getElementById(id);
const colors = ["#a9c5eb", "#afa4cd", "#e0b481", "#93bcae", "#e398a1"];
const time = (ms) => new Date(ms).toLocaleTimeString();
let view,
  metrics,
  selection = "",
  streamLive = false,
  lastContact = 0,
  contactTimeout;
const seen = new Set(),
  slots = new Map();
function svg(tag, attributes, parent, label) {
  const el = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [key, value] of Object.entries(attributes))
    el.setAttribute(key, value);
  if (label !== undefined) el.textContent = label;
  parent.append(el);
  return el;
}
function label(parent, x, y, text, color = "#dde0e7", size = 12) {
  return svg("text", { x, y, fill: color, "font-size": size }, parent, text);
}
function box(parent, x, y, width, height, fill, stroke) {
  return svg("rect", { x, y, width, height, rx: 5, fill, stroke }, parent);
}
function choose(node) {
  selection = node;
  $("worker").value = node;
  drawMap(false);
  drawCharts();
}
function drawExplorer(names, nodes, pods) {
  $("nodeCount").textContent =
    names.length + (names.length === 1 ? " node" : " nodes");
  $("overview").setAttribute("aria-pressed", String(!selection));
  const list = $("nodeList");
  const fragment = document.createDocumentFragment();
  for (const name of names) {
    const item = document.createElement("div");
    item.className = "nodeitem";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "nodebtn";
    const indicator = document.createElement("i");
    indicator.className = nodes[name].ready ? "indicator" : "indicator wait";
    button.append(indicator, name);
    button.title = name + " · " + (nodes[name].ready ? "Ready" : "Not ready");
    button.setAttribute("aria-pressed", String(name === selection));
    button.onclick = () => choose(name === selection ? "" : name);
    const children = document.createElement("ul");
    children.className = "children";
    for (const [pod, details] of Object.entries(pods).filter(
      ([, p]) => p.node === name,
    )) {
      const row = document.createElement("li");
      const dot = document.createElement("i");
      dot.className =
        details.ready && !details.deleting ? "indicator" : "indicator wait";
      const text = document.createElement("span");
      text.textContent = pod;
      row.title =
        pod +
        " · " +
        (details.deleting
          ? "Terminating"
          : details.ready
            ? "Ready"
            : details.phase);
      row.append(dot, text);
      children.append(row);
    }
    item.append(button, children);
    fragment.append(item);
  }
  list.replaceChildren(fragment);
}
function drawMap(animate) {
  const graph = $("topology"),
    r = view.resources,
    nodes = r.nodes || {},
    pods = r.pods || {};
  let scene = graph.querySelector(".scene"),
    flights = graph.querySelector(".flights");
  if (!scene) {
    scene = svg("g", { class: "scene" }, graph);
    flights = svg("g", { class: "flights", "pointer-events": "none" }, graph);
  }
  scene.replaceChildren();
  const defs = svg("defs", {}, scene),
    marker = svg(
      "marker",
      {
        id: "arrow",
        viewBox: "0 0 10 10",
        refX: 9,
        refY: 5,
        markerWidth: 6,
        markerHeight: 6,
        orient: "auto",
      },
      defs,
    );
  svg("path", { d: "M0,0 L10,5 L0,10", fill: "#799dcc" }, marker);
  const names = Object.keys(nodes);
  for (const name of names) if (!slots.has(name)) slots.set(name, slots.size);
  names.sort((a, b) => slots.get(a) - slots.get(b));
  if (selection && !names.includes(selection)) selection = "";
  const selector = $("worker");
  selector.replaceChildren(
    new Option("All nodes", ""),
    ...names.map((n) => new Option(n, n)),
  );
  selector.value = selection;
  drawExplorer(names, nodes, pods);
  const waiting = Object.values(pods).some((p) => !p.node || !nodes[p.node]);
  const groups = waiting ? [...names, "Awaiting placement"] : names;
  const positions = {},
    services = {},
    claims = {},
    edges = new Map();
  const cards = svg("g", {}, scene);
  label(cards, 10, 15, "SERVICES", "#989da9", 10);
  label(cards, 175, 15, "NODES / PODS", "#989da9", 10);
  label(cards, 530, 15, "STORAGE", "#989da9", 10);
  let top = 28;
  for (const node of groups) {
    const entries = Object.entries(pods).filter(([, p]) =>
      node === "Awaiting placement"
        ? !p.node || !nodes[p.node]
        : p.node === node,
    );
    const h = 58 + Math.max(1, entries.length) * 49;
    const group = svg("g", {}, cards);
    box(
      group,
      175,
      top,
      335,
      h,
      selection === node ? "#232d3c" : "#202227",
      selection === node ? "#83b5ff" : "#50535e",
    );
    label(group, 187, top + 22, node, "#f0f1f4", 14);
    if (nodes[node]) {
      group.setAttribute("tabindex", "0");
      group.setAttribute("role", "button");
      group.setAttribute("aria-label", "Filter metrics for " + node);
      group.classList.add("nodehit");
      group.onclick = () => choose(selection === node ? "" : node);
      group.onkeydown = (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          group.onclick();
        }
      };
    }
    label(
      group,
      187,
      top + 39,
      nodes[node]
        ? nodes[node].ready
          ? "● Ready"
          : "○ Not ready"
        : "Pending scheduler",
      "#adb5be",
      10,
    );
    entries.forEach(([name, p], i) => {
      const y = top + 47 + i * 49;
      box(group, 190, y, 304, 41, "#2a2c33", p.ready ? "#444852" : "#ab7949");
      const count = (view.run?.requests || []).filter(
        (row) => row.pod === name,
      ).length;
      label(group, 201, y + 16, name, "#e4e6ed", 11);
      label(
        group,
        201,
        y + 32,
        (p.deleting ? "Terminating" : p.ready ? "Ready" : p.phase) +
          (!audience && p.app === "grace" ? "   ·   " + count + " logins" : ""),
        "#b2b5be",
        10,
      );
      positions[name] = [190, y + 20];
    });
    if (!entries.length)
      label(group, 190, top + 71, "No Grace workloads", "#a0a3ae", 12);
    top += h + 18;
  }
  Object.keys(r.services || {}).forEach((svc, i) => {
    const y = 37 + i * 70;
    services[svc] = [129, y + 19];
    box(cards, 10, y, 119, 39, "#252b35", "#536a88");
    label(cards, 20, y + 24, svc + " service", "#cbdefa", 12);
  });
  const usedClaims = [
    ...new Set(Object.values(pods).flatMap((p) => p.claims || [])),
  ];
  usedClaims.forEach((claim, i) => {
    const y = 57 + i * 69;
    claims[claim] = [530, y + 19];
    box(cards, 530, y, 134, 43, "#2b2930", "#6f677b");
    label(cards, 536, y + 16, claim, "#d5cddd", 10);
    label(
      cards,
      536,
      y + 32,
      r.claims?.[claim]?.phase || "Claim pending",
      "#b7adbf",
      10,
    );
  });
  const curve = (a, b) =>
    "M" +
    a[0] +
    "," +
    a[1] +
    " C" +
    (a[0] + b[0]) / 2 +
    "," +
    a[1] +
    " " +
    (a[0] + b[0]) / 2 +
    "," +
    b[1] +
    " " +
    b[0] +
    "," +
    b[1];
  // Paint connections above node backgrounds but below endpoint labels.
  for (const slice of Object.values(r.slices || {}))
    for (const pod of slice.pods) {
      const a = services[slice.service],
        b = positions[pod],
        key = slice.service + "/" + pod;
      if (a && b && !edges.has(key)) {
        const d = curve(a, b);
        edges.set(key, d);
        svg(
          "path",
          {
            d,
            fill: "none",
            stroke: "#799dcc",
            "stroke-width": 1.6,
            "marker-end": "url(#arrow)",
          },
          scene,
        );
      }
    }
  for (const [pod, p] of Object.entries(pods))
    for (const c of p.claims || []) {
      if (positions[pod] && claims[c])
        svg(
          "path",
          {
            d: curve([494, positions[pod][1]], claims[c]),
            stroke: "#a69bb3",
            "stroke-dasharray": "4 3",
            fill: "none",
          },
          scene,
        );
    }
  graph.setAttribute(
    "viewBox",
    "0 0 678 " +
      Math.max(
        top,
        90 + Object.keys(services).length * 65,
        80 + usedClaims.length * 69,
      ),
  );
  if (!groups.length)
    label(scene, 177, 60, "Waiting for cluster snapshot", "#9db6b5");
  for (const row of view.run?.requests || []) {
    const d = edges.get("app/" + row.pod);
    if (!row.pod || seen.has(row.id) || !d) continue;
    if (
      animate &&
      d &&
      !matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      const dot = svg("g", { visibility: "hidden" }, flights);
      const index = view.run.requests.indexOf(row);
      svg("circle", { r: 8, fill: "#f1c17c" }, dot);
      svg(
        "text",
        {
          x: 0,
          y: 3,
          "text-anchor": "middle",
          "font-size": 9,
          fill: "#272322",
        },
        dot,
        String(index + 1),
      );
      const motion = svg(
        "animateMotion",
        { dur: "1.1s", path: d, begin: "indefinite", fill: "freeze" },
        dot,
      );
      motion.addEventListener(
        "beginEvent",
        () => dot.setAttribute("visibility", "visible"),
        { once: true },
      );
      motion.beginElementAt(index * 0.055);
      motion.addEventListener("endEvent", () => dot.remove(), { once: true });
    }
    seen.add(row.id);
  }
  const app = r.deployments?.app,
    hpa = r.hpa?.app;
  $("summary").textContent =
    names.length +
    " nodes · " +
    (app ? app.available + " / " + app.desired : "—") +
    " app replicas · HPA CPU " +
    (hpa?.cpu ?? "—") +
    "% / " +
    (hpa?.target ?? "—") +
    "%";
}
function drawRun() {
  const run = view.run,
    timeline = $("timeline");
  timeline.replaceChildren();
  $("distribution").replaceChildren();
  if (!run) {
    $("runNote").textContent = $("runTime").textContent = "";
    $("totals").textContent = "Waiting for a login burst.";
    return;
  }
  const rows = run.requests,
    done = rows.map((r) => r.client).filter(Boolean);
  const success = done.filter((c) => c.status >= 200 && c.status < 300).length,
    timeout = done.filter((c) => c.timeout).length;
  const totals = $("totals");
  totals.replaceChildren();
  const result = document.createElement("div");
  result.className = "result-counts";
  for (const [caption, count] of [
    ["Sent", rows.length],
    ["OK", success],
    ["Failed", done.length - success - timeout],
    ["Timeout", timeout],
  ]) {
    const group = document.createElement("div"),
      value = document.createElement("b"),
      label = document.createElement("small");
    value.textContent = count;
    label.textContent = caption;
    if (count && (caption === "Failed" || caption === "Timeout"))
      value.className = "bad";
    group.append(value, " ", label);
    result.append(group);
  }
  totals.append(result);
  $("runTime").textContent = time(run.started);
  const t0 = Math.min(...done.map((x) => x.start)),
    span = Math.max(1, ...done.map((x) => x.end - t0));
  rows.forEach((row, i) => {
    const lane = document.createElement("div");
    lane.className = "lane";
    const num = document.createElement("span");
    num.textContent = String(i + 1).padStart(2, "0");
    const track = document.createElement("div");
    track.className = "track";
    const result = document.createElement("span"),
      c = row.client;
    if (c) {
      const bar = document.createElement("span");
      bar.className =
        "bar" + (c.status >= 200 && c.status < 300 ? "" : " error");
      bar.style.left = (100 * (c.start - t0)) / span + "%";
      bar.style.width = (100 * (c.end - c.start)) / span + "%";
      track.append(bar);
      result.textContent = c.timeout ? "timeout" : (c.status ?? "network");
    } else result.textContent = "pending";
    lane.title =
      "Request " +
      (i + 1) +
      " · " +
      (row.pod || "Pod unknown") +
      (c ? " · " + (c.end - c.start) + " ms" : "");
    lane.append(num, track, result);
    timeline.append(lane);
  });
  const counts = new Map();
  for (const row of rows) {
    if (row.pod) counts.set(row.pod, (counts.get(row.pod) || 0) + 1);
  }
  for (const [pod, count] of counts) {
    const item = document.createElement("li");
    item.className = "distribution-row";
    const value = document.createElement("b");
    value.textContent = count;
    const meter = document.createElement("i");
    meter.style.setProperty("--share", (count / rows.length) * 100 + "%");
    item.append(pod, value, meter);
    $("distribution").append(item);
  }
  $("runNote").textContent = done.length
    ? Math.round(span) +
      " ms client window · " +
      rows.filter((r) => r.pod).length +
      " attributed to a Pod"
    : "Waiting for client results.";
}
const panels = [
  ["CPU", "%", ["cpu"]],
  ["Memory", "%", ["memory"]],
  ["Root disk used", "%", ["root"]],
  ["Network · in / out", "KiB/s", ["netIn", "netOut"], 1024],
  ["PostgreSQL · free / capacity", "GiB", ["pvc", "capacity"], 2 ** 30],
  ["Replicas · desired / available", "pods", ["desired", "available"]],
  ["Answer submissions", "/ min", ["answers"]],
  ["Answer failure rate", "%", ["failures"]],
  ["Answer latency · mean", "ms", ["latency"]],
];
function plot(el, lines, start, end, unit, step = 15) {
  el.replaceChildren();
  el.setAttribute("viewBox", "0 0 300 65");
  el.setAttribute("preserveAspectRatio", "none");
  const values = lines
    .flatMap((line) => line.points.map((p) => p[1]))
    .filter(Number.isFinite);
  const max = Math.max(1, ...values);
  const x = (t) => ((t - start) / (end - start)) * 296 + 2,
    y = (v) => 59 - (v / max) * 52;
  for (const yy of [7, 59])
    svg("line", { x1: 0, x2: 300, y1: yy, y2: yy, stroke: "#363941" }, el);
  if (
    view.run &&
    view.run.started / 1000 >= start &&
    view.run.started / 1000 <= end
  )
    svg(
      "line",
      {
        x1: x(view.run.started / 1000),
        x2: x(view.run.started / 1000),
        y1: 2,
        y2: 62,
        stroke: "#e0bd7d",
        "stroke-dasharray": "2 3",
      },
      el,
    );
  lines.forEach((l, i) => {
    let path = "",
      prev = -Infinity;
    for (const [t, v] of l.points) {
      if (!Number.isFinite(v)) {
        prev = -Infinity;
        continue;
      }
      if (t < start || t > end) continue;
      path += (t - prev > step * 1.6 ? " M" : " L") + x(t) + "," + y(v);
      prev = t;
    }
    const line = svg(
      "path",
      {
        d: path,
        fill: "none",
        stroke: colors[i % colors.length],
        "stroke-width": 2,
      },
      el,
    );
    svg("title", {}, line, l.name + " · " + unit);
  });
  return values.length > 0;
}
function drawFlow() {
  const end = metrics?.end ?? Date.now() / 1000;
  const lines = [["podIn", "Inbound", "flowIn"], ["podOut", "Outbound", "flowOut"]]
    .map(([key, name, id]) => {
      const points = (metrics?.series?.[key]?.[0]?.values || [])
        .map(([t, v]) => [t, Number(v) / 1024])
        .filter(([t]) => t >= end - 300);
      const last = points.at(-1);
      const current = last && last[0] === end && Number.isFinite(last[1]) && Date.now() / 1000 - end < 45;
      $(id).textContent = current ? last[1].toFixed(1) + " KiB/s" : "—";
      return { name, points, current };
    });
  plot($("flowPlot"), lines, end - 300, end, "KiB/s");
  $("flowTime").textContent = lines.every(l => l.current)
    ? "5 min trend · updated " + time(end * 1000)
    : "Waiting for current network samples.";
}

const historyKey = "grace.ops.pod-activity.v1";
let podHistory = [];
try {
  const saved = JSON.parse(localStorage.getItem(historyKey));
  if (Array.isArray(saved))
    podHistory = saved.filter(e => e && typeof e.id === "string" && typeof e.pod === "string"
      && ["Created", "Deleted"].includes(e.action) && Number.isFinite(e.time)).slice(-100);
} catch { /* Storage is optional in private browsing. */ }

function drawPodEvents(incoming) {
  const known = new Set(podHistory.map(e => e.id));
  const added = incoming.filter(e => !known.has(e.id));
  if (added.length) {
    podHistory = podHistory.concat(added).sort((a, b) => a.time - b.time).slice(-100);
    try { localStorage.setItem(historyKey, JSON.stringify(podHistory)); } catch {}
    const last = added.at(-1);
    $("podAnnounce").textContent = last.action + " " + last.pod;
  }
  $("podEmpty").hidden = podHistory.length > 0;
  const rows = document.createDocumentFragment();
  for (const e of [...podHistory].reverse()) {
    const li = document.createElement("li"), action = document.createElement("b"),
      stamp = document.createElement("time"), pod = document.createElement("code");
    action.className = e.action === "Created" ? "created" : "deleted";
    action.textContent = e.action;
    stamp.dateTime = new Date(e.time).toISOString();
    stamp.textContent = new Date(e.time).toLocaleString(undefined, {month:"short", day:"numeric", hour:"2-digit", minute:"2-digit", second:"2-digit"});
    pod.textContent = e.pod;
    li.append(action, stamp, pod);
    rows.append(li);
  }
  $("podEvents").replaceChildren(rows);
}
drawPodEvents([]);

function drawResourceStrip() {
  const strip = $("quickMetrics");
  strip.replaceChildren();
  const end = metrics?.end ?? Date.now() / 1000;
  for (const [key, name] of [
    ["cpu", "CPU"],
    ["memory", "Memory"],
    ["root", "Root disk"],
  ]) {
    const data = (metrics?.series?.[key] || []).filter(
      (s) => !selection || s.metric.node === selection,
    );
    const peaks = new Map();
    for (const line of data)
      for (const [t, v] of line.values) {
        const n = Number(v);
        if (Number.isFinite(n)) peaks.set(t, Math.max(peaks.get(t) ?? 0, n));
      }
    const points = [...peaks].sort((a, b) => a[0] - b[0]);
    const card = document.createElement("div");
    card.className = "quick";
    const title = document.createElement("h3");
    title.textContent = name + (selection ? "" : " · peak node");
    const value = document.createElement("strong");
    const last = points.at(-1);
    value.textContent =
      last && last[0] >= end - 30 ? last[1].toFixed(1) + "%" : "—";
    card.append(title, value);
    const chart = svg(
      "svg",
      { role: "img", "aria-label": name + " trend" },
      card,
    );
    plot(chart, [{ name, points }], end - 900, end, "%");
    strip.append(card);
  }
}
function drawCharts() {
  drawFlow();
  drawResourceStrip();
  if (audience) return;
  const parent = $("charts");
  parent.replaceChildren();
  if (metrics)
    $("range").textContent =
      time((metrics.end - 900) * 1000) + " – " + time(metrics.end * 1000);
  for (const [name, unit, keys, div = 1] of panels) {
    const card = document.createElement("article");
    card.className = "chart";
    const h = document.createElement("h3");
    h.textContent = name;
    const value = document.createElement("p");
    const graph = svg("svg", { "aria-label": name, role: "img" }, card);
    const lines = keys.flatMap((key) =>
      (metrics?.series?.[key] || [])
        .filter(
          (l) => !selection || !l.metric.node || l.metric.node === selection,
        )
        .map((l) => ({
          name: [key, l.metric.node].filter(Boolean).join(" "),
          points: l.values.map(([t, v]) => [t, Number(v) / div]),
        })),
    );
    const end = metrics?.end ?? Date.now() / 1000;
    const data = plot(graph, lines, end - 900, end, unit);
    const latest = lines.map((l) => {
      const p = l.points.at(-1);
      return p && p[0] >= end - 30 && Number.isFinite(p[1])
        ? p[1].toFixed(1)
        : "—";
    });
    value.textContent = data
      ? latest.join(" / ") + " " + unit
      : !metrics || keys.some((k) => metrics.series[k] === null)
        ? "Metrics unavailable"
        : keys.includes("pvc")
          ? "Volume statistics unavailable"
          : "No samples";
    card.prepend(h, value);
    if (data) {
      const legend = document.createElement("div");
      legend.className = "legend";
      lines.forEach((l, i) => {
        const item = document.createElement("span");
        item.style.setProperty("--series", colors[i % colors.length]);
        item.textContent = l.name;
        legend.append(item);
      });
      card.append(legend);
    }
    parent.append(card);
  }
}
function drawHealth() {
  const arr = view.probes || [],
    last = arr.at(-1),
    end = Date.now() / 1000;
  $("health").textContent = last
    ? (Date.now() - last.time > 25000 ? "Probe stale · " : "") +
      (last.ok ? "Ready" : "Failed") +
      " · " +
      last.ms +
      " ms · " +
      time(last.time) +
      " · presenter probe"
    : "Waiting for the presenter’s probe.";
  const el = $("probeplot");
  plot(
    el,
    [{ name: "Probe latency", points: arr.map((p) => [p.time / 1000, p.ms]) }],
    end - 900,
    end,
    "ms",
    10,
  );
  for (const p of arr.filter((p) => p.time / 1000 >= end - 900))
    svg(
      "circle",
      {
        cx: ((p.time / 1000 - end + 900) / 900) * 296 + 2,
        cy: 61,
        r: 2,
        fill: p.ok ? colors[0] : "#ee896b",
      },
      el,
    );
  if (view.uptime_url) $("uptime").href = view.uptime_url;
}
function staleProbe() {
  if (view) drawHealth();
}
setInterval(staleProbe, 10000);
let loadPending = false;
let loadError = "";
function drawLoadTest() {
  const test = view?.load_test;
  $("loadTest").hidden = !test;
  if (!test) return;
  const ready = view.resources.deployments?.app?.available;
  const running = test.phase === "running";
  const messages = {
    idle: "Up to 3 min of application CPU load.",
    running: "Applying CPU load…",
    reached: "Reached 5. Load stopped.",
    stopped: "Stopped by presenter.",
    timed_out: "Time limit reached. Load stopped.",
    interrupted: "Cluster data unavailable. Load stopped.",
  };
  $("loadStatus").textContent =
    (ready ?? "—") + " / 5 ready · " + (loadError || messages[test.phase]);
  const button = $("loadButton");
  button.textContent = running ? "Stop load test" : "Start load test";
  button.disabled =
    loadPending ||
    !streamLive ||
    (!running &&
      (ready == null ||
        ready >= 5 ||
        !view.synced.deployments ||
        !view.synced.hpa));
}
$("loadButton").onclick = async () => {
  if (loadPending) return;
  loadPending = true;
  loadError = "";
  const command = view.load_test.phase === "running" ? "stop" : "start";
  drawLoadTest();
  try {
    const result = await fetch("/api/load-test", {
      method: "POST",
      headers: { "Content-Type": "application/json", ...(localStorage.getItem("grace.presenter") && {Authorization: "Bearer " + localStorage.getItem("grace.presenter")}) },
      body: JSON.stringify(command),
    });
    if (!result.ok)
      loadError =
        result.status === 401 ? "Open your private presenter link to unlock this browser." : result.status === 409
          ? "Waiting for HPA or scale-down."
          : "Load control unavailable (" + result.status + ").";
  } catch {
    loadError = "Dashboard disconnected.";
  } finally {
    loadPending = false;
    drawLoadTest();
  }
};
function showSync() {
  drawLoadTest();
  const ok =
    streamLive &&
    Object.keys(view?.synced || {}).length === 7 &&
    Object.values(view.synced).every(Boolean);
  $("sync").dataset.live = String(ok);
  $("sync").textContent = ok ? "Live" : "Sync interrupted / connecting";
  $("updated").textContent = lastContact
    ? "Last contact " + time(lastContact)
    : "Waiting for dashboard";
  $("updated").title = view?.updated
    ? "Last Kubernetes sync " + time(view.updated)
    : "Waiting for Kubernetes";
}
function selectView() {
  if (audience) return;
  const links = [...document.querySelectorAll(".toolbar nav a")];
  const active = links.find((a) => a.hash === location.hash) || links[0];
  for (const link of links) {
    $(link.hash.slice(1)).hidden = link !== active;
    if (link === active) link.setAttribute("aria-current", "page");
    else link.removeAttribute("aria-current");
  }
}
window.addEventListener("hashchange", selectView);
selectView();
$("overview").onclick = () => {
  if (view) choose("");
};
const feed = new EventSource("/api/events");
function recordContact() {
  lastContact = Date.now();
  streamLive = true;
  clearTimeout(contactTimeout);
  contactTimeout = setTimeout(contactLost, 12000);
  showSync();
}
function contactLost() {
  clearTimeout(contactTimeout);
  streamLive = false;
  showSync();
}
feed.addEventListener("heartbeat", recordContact);
feed.onmessage = (e) => {
  const next = JSON.parse(e.data),
    changedRun = next.run?.started !== view?.run?.started;
  if (changedRun) seen.clear();
  const topologyChanged =
    !view || JSON.stringify(next.resources) !== JSON.stringify(view.resources);
  const burstChanged =
    !view || JSON.stringify(next.run) !== JSON.stringify(view.run);
  const wasLive = streamLive;
  if (!view || JSON.stringify(next.pod_events) !== JSON.stringify(view.pod_events))
    drawPodEvents(next.pod_events || []);
  view = next;
  recordContact();
  if (topologyChanged || burstChanged) drawMap(wasLive);
  if (burstChanged) {
    drawRun();
    drawCharts();
  }
  drawHealth();
};
feed.onerror = contactLost;
$("worker").onchange = (e) => choose(e.target.value);
async function fetchMetrics() {
  try {
    const res = await fetch("/api/metrics");
    if (!res.ok) throw Error();
    metrics = await res.json();
  } catch {
    metrics = null;
  }
  if (view) {
    drawCharts();
    drawHealth();
  }
  setTimeout(fetchMetrics, 15000);
}
fetchMetrics();

if (!audience && location.port === "19090") import("/logs.js");
