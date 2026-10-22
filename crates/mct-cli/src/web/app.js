// Progressive enhancement only: every page and action works without this script.
(function () {
  "use strict";

  // "/" focuses the first search box; Escape leaves it.
  document.addEventListener("keydown", function (event) {
    var target = event.target;
    var typing = target && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA");
    if (event.key === "/" && !typing && !event.ctrlKey && !event.metaKey) {
      var box = document.querySelector("input[type=search]");
      if (box) { event.preventDefault(); box.focus(); box.select(); }
    } else if (event.key === "Escape" && typing) {
      target.blur();
    }
  });

  // Live filter of the rows already on the page. The server filter stays authoritative.
  var form = document.querySelector("form[data-filter-form]");
  var box = form && form.querySelector("input[name=q]");
  if (box) {
    var rows = Array.prototype.slice.call(document.querySelectorAll("[data-search]"));
    box.addEventListener("input", function () {
      var needle = box.value.trim().toLowerCase();
      rows.forEach(function (row) {
        row.hidden = needle !== "" && row.getAttribute("data-search").indexOf(needle) === -1;
      });
    });
  }

  // Review decisions without losing the scroll position.
  document.addEventListener("submit", function (event) {
    var f = event.target;
    var action = f instanceof HTMLFormElement ? f.getAttribute("action") : null;
    if ((action !== "review" && action !== "lang") || !window.fetch) { return; }
    var submitter = event.submitter;
    if (!submitter || !submitter.name) { return; }
    event.preventDefault();
    var data = new URLSearchParams(new FormData(f));
    data.set(submitter.name, submitter.value);
    Array.prototype.forEach.call(f.querySelectorAll("button"), function (b) { b.disabled = true; });
    fetch(action, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: data.toString(),
      credentials: "same-origin",
      redirect: "manual"
    }).then(function (response) {
      if (response.type === "opaqueredirect" || response.ok) {
        window.location.reload();
      } else {
        return response.text().then(function (text) { throw new Error(text); });
      }
    }).catch(function (error) {
      var es = document.documentElement.lang === "es";
      window.alert((es ? "No se pudo guardar la decisión: " : "The decision could not be saved: ") + error.message);
      Array.prototype.forEach.call(f.querySelectorAll("button"), function (b) { b.disabled = false; });
    });
  });

  // Note graph: small force layout on a canvas, data from a data attribute.
  var holder = document.getElementById("graph");
  if (!holder) { return; }
  var graph;
  try { graph = JSON.parse(holder.getAttribute("data-graph")); } catch (e) { return; }
  var canvas = holder.querySelector("canvas");
  var ctx = canvas.getContext("2d");
  var dpr = window.devicePixelRatio || 1;
  var width = 0, height = 0;
  function resize() {
    width = holder.clientWidth; height = holder.clientHeight;
    canvas.width = width * dpr; canvas.height = height * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }
  resize();
  window.addEventListener("resize", function () { resize(); draw(); });
  var nodes = graph.nodes.map(function (n, i) {
    var angle = (i / graph.nodes.length) * 2 * Math.PI;
    return { t: n.t, h: n.h, f: n.f, d: n.d, x: width / 2 + Math.cos(angle) * 120 * (n.f ? 0 : 1), y: height / 2 + Math.sin(angle) * 120 * (n.f ? 0 : 1), vx: 0, vy: 0 };
  });
  var edges = graph.edges;
  var style = getComputedStyle(document.documentElement);
  function color(name) { return style.getPropertyValue(name).trim() || "#888"; }
  var steps = 0, drag = null, hover = null;
  function tick() {
    var n = nodes.length, i, j;
    for (i = 0; i < n; i++) {
      for (j = i + 1; j < n; j++) {
        var dx = nodes[i].x - nodes[j].x, dy = nodes[i].y - nodes[j].y;
        var d2 = dx * dx + dy * dy + 0.01, f = 2200 / d2, d = Math.sqrt(d2);
        nodes[i].vx += dx / d * f; nodes[i].vy += dy / d * f;
        nodes[j].vx -= dx / d * f; nodes[j].vy -= dy / d * f;
      }
    }
    edges.forEach(function (e) {
      var a = nodes[e[0]], b = nodes[e[1]];
      var dx = b.x - a.x, dy = b.y - a.y, d = Math.sqrt(dx * dx + dy * dy) + 0.01, f = (d - 70) * 0.02;
      a.vx += dx / d * f; a.vy += dy / d * f; b.vx -= dx / d * f; b.vy -= dy / d * f;
    });
    nodes.forEach(function (p) {
      p.vx += (width / 2 - p.x) * 0.004; p.vy += (height / 2 - p.y) * 0.004;
      if (p !== drag) { p.x += p.vx * 0.5; p.y += p.vy * 0.5; }
      p.vx *= 0.8; p.vy *= 0.8;
      p.x = Math.max(8, Math.min(width - 8, p.x)); p.y = Math.max(8, Math.min(height - 8, p.y));
    });
  }
  function radius(p) { return 4 + Math.min(10, Math.sqrt(p.d)); }
  function draw() {
    ctx.clearRect(0, 0, width, height);
    ctx.strokeStyle = color("--line"); ctx.lineWidth = 1; ctx.beginPath();
    edges.forEach(function (e) { ctx.moveTo(nodes[e[0]].x, nodes[e[0]].y); ctx.lineTo(nodes[e[1]].x, nodes[e[1]].y); });
    ctx.stroke();
    nodes.forEach(function (p) {
      ctx.fillStyle = p.f ? color("--warn") : color("--accent");
      ctx.beginPath(); ctx.arc(p.x, p.y, radius(p), 0, 2 * Math.PI); ctx.fill();
    });
    if (hover) {
      ctx.font = "13px system-ui,sans-serif"; ctx.fillStyle = color("--text");
      ctx.fillText(hover.t, Math.min(hover.x + 10, width - 160), hover.y - 10);
    }
  }
  function frame() {
    if (steps < 300 || drag) { tick(); steps++; draw(); requestAnimationFrame(frame); } else { draw(); }
  }
  function at(event) {
    var box = canvas.getBoundingClientRect();
    var x = event.clientX - box.left, y = event.clientY - box.top, found = null;
    nodes.forEach(function (p) { if (Math.hypot(p.x - x, p.y - y) <= radius(p) + 3) { found = p; } });
    return { x: x, y: y, node: found };
  }
  var moved = false;
  canvas.addEventListener("pointerdown", function (event) {
    var hit = at(event); moved = false;
    if (hit.node) { drag = hit.node; canvas.setPointerCapture(event.pointerId); steps = 0; frame(); }
  });
  canvas.addEventListener("pointermove", function (event) {
    var hit = at(event);
    if (drag) { drag.x = hit.x; drag.y = hit.y; moved = true; }
    else { hover = hit.node; canvas.style.cursor = hit.node ? "pointer" : "default"; draw(); }
  });
  canvas.addEventListener("pointerup", function () {
    if (drag && !moved) { window.location.href = drag.h; }
    drag = null;
  });
  frame();
})();
