/* sheprd.org/docs: search palette, copy buttons, contents menu, and the current section. */
(function () {
  "use strict";
  var $$ = function (s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); };
  var esc = function (s) { return String(s).replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; }); };

  /* Comments, strings and a few keywords; the text itself is never interpreted. */
  var RULES = {
    lua: /(--.*$)|("(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*')|\b(local|function|end|if|then|not|else|elseif|for|in|do|return|and|or|nil|true|false)\b|\b(\d[\d_.]*)\b/gm,
    luau: null,
    toml: /(#.*$)|("(?:[^"\\\n]|\\.)*"|'[^'\n]*')|^(\s*\[[^\]\n]+\])|\b(true|false|\d[\d_.]*)\b/gm,
    sh: /(#.*$)|("(?:[^"\\\n]|\\.)*"|'[^'\n]*')|^(\$ )/gm,
    powershell: /(#.*$)|("(?:[^"`\n]|`.)*"|'[^'\n]*')/gm
  };
  RULES.luau = RULES.lua; RULES.bash = RULES.sh; RULES.shell = RULES.sh; RULES.console = RULES.sh;
  function highlight(code) {
    var lang = (code.className.match(/language-(\w+)/) || [])[1], re = RULES[lang];
    if (!re) return;
    var src = code.textContent, out = "", last = 0, m;
    re.lastIndex = 0;
    while ((m = re.exec(src))) {
      if (!m[0]) { re.lastIndex++; continue; }
      out += esc(src.slice(last, m.index));
      var cls = m[1] ? "c" : m[2] ? "s" : m[3] ? "k" : "n";
      out += '<span class="t-' + cls + '">' + esc(m[0]) + "</span>";
      last = m.index + m[0].length;
    }
    code.innerHTML = out + esc(src.slice(last));
  }
  $$("pre > code").forEach(function (code) {
    highlight(code);
    var b = document.createElement("button");
    b.type = "button"; b.className = "copy"; b.textContent = "Copy";
    b.addEventListener("click", function () {
      var done = function () { b.textContent = "Copied"; setTimeout(function () { b.textContent = "Copy"; }, 1200); };
      if (navigator.clipboard) navigator.clipboard.writeText(code.textContent).then(done, function () {});
    });
    code.parentNode.appendChild(b);
  });

  var menu = document.querySelector("[data-menu]");
  if (menu) menu.addEventListener("click", function () {
    var open = document.documentElement.classList.toggle("menu-open");
    menu.setAttribute("aria-expanded", open);
  });

  var tocLinks = $$("[data-toc]");
  if (tocLinks.length && "IntersectionObserver" in window) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (!e.isIntersecting) return;
        tocLinks.forEach(function (l) { l.classList.toggle("on", l.dataset.toc === e.target.id); });
      });
    }, { rootMargin: "-15% 0px -75% 0px" });
    tocLinks.forEach(function (l) { var h = document.getElementById(l.dataset.toc); if (h) io.observe(h); });
  }

  /* Search: page titles first, then section titles, then section text. The
     index holds every section's full text, so it loads on first use. */
  var entries = [], loading = null;
  var failed = false;
  /* A failed load clears itself, so opening search again retries. */
  function loadIndex(done) {
    if (window.SHEPRD_DOCS_INDEX) return done();
    if (!loading) {
      loading = [];
      failed = false;
      var s = document.createElement("script");
      s.src = "/docs/search.js";
      var settle = function (ok) {
        var waiting = loading;
        loading = null;
        failed = !ok;
        if (!ok) s.remove();
        waiting.forEach(function (f) { f(); });
      };
      s.onload = function () { settle(!!window.SHEPRD_DOCS_INDEX); };
      s.onerror = function () { settle(false); };
      document.head.appendChild(s);
    }
    loading.push(done);
  }
  function indexEntries() {
    if (entries.length || !window.SHEPRD_DOCS_INDEX) return;
    window.SHEPRD_DOCS_INDEX.forEach(function (p) {
      entries.push({ title: p.t, sub: p.g + " · " + p.b, url: p.u, hay: [p.t.toLowerCase(), p.b.toLowerCase(), ""] });
      p.s.forEach(function (s) {
        entries.push({ title: s.title, sub: p.t, url: p.u + "#" + s.id, hay: ["", s.title.toLowerCase(), s.text.toLowerCase()], text: s.text });
      });
    });
  }
  function score(e, words) {
    var total = 0;
    for (var i = 0; i < words.length; i++) {
      var w = words[i];
      var s = e.hay[0].indexOf(w) >= 0 ? 30 : e.hay[1].indexOf(w) >= 0 ? 12 : e.hay[2].indexOf(w) >= 0 ? 3 : 0;
      if (!s) return 0;
      total += s;
    }
    return total;
  }
  function snippet(e, words) {
    if (!e.text) return e.sub;
    var at = -1;
    words.forEach(function (w) { var i = e.hay[2].indexOf(w); if (i >= 0 && (at < 0 || i < at)) at = i; });
    if (at < 0) return e.sub;
    return e.sub + " · " + (at > 30 ? "…" : "") + e.text.slice(Math.max(0, at - 30), at + 90);
  }

  /* A modal <dialog>: the page behind it is inert, so focus stays inside, and
     closing it returns focus to whatever opened it. */
  var dialog, input, list, results = [], active = 0, opener = null;
  function draw() {
    indexEntries();
    var words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
    results = words.length
      ? entries.map(function (e) { return [score(e, words), e]; }).filter(function (r) { return r[0]; })
          .sort(function (a, b) { return b[0] - a[0]; }).slice(0, 12).map(function (r) { return r[1]; })
      : entries.filter(function (e) { return !e.text; }).slice(0, 12);
    active = 0;
    if (!entries.length) {
      list.innerHTML = '<li class="pal-none">' + (failed ? "Search could not load. Close it and try again." : "Loading…") + "</li>";
      return;
    }
    list.innerHTML = results.length ? results.map(function (e, n) {
      return '<li><a href="' + esc(e.url) + '"' + (n ? "" : ' class="on"') + "><b>" + esc(e.title) + "</b><span>" + esc(snippet(e, words)) + "</span></a></li>";
    }).join("") : '<li class="pal-none">No page matches.</li>';
  }
  function mark() { $$("a", list).forEach(function (a, n) { a.classList.toggle("on", n === active); if (n === active) a.scrollIntoView({ block: "nearest" }); }); }
  function close() { if (dialog && dialog.open) dialog.close(); }
  function build() {
    dialog = document.createElement("dialog");
    dialog.id = "palette";
    dialog.setAttribute("aria-label", "Search the docs");
    dialog.innerHTML = '<div class="pal-box"><input type="search" placeholder="Search the docs…" aria-label="Search the docs"><ul></ul></div>';
    document.body.appendChild(dialog);
    input = dialog.querySelector("input"); list = dialog.querySelector("ul");
    /* A click on the backdrop lands on the dialog itself, outside its box. */
    dialog.addEventListener("click", function (e) { if (e.target === dialog || e.target.closest("a")) close(); });
    dialog.addEventListener("close", function () { if (opener && opener.focus) opener.focus(); opener = null; });
    /* Tab and Shift-Tab wrap at the ends, so focus never leaves the dialog. */
    dialog.addEventListener("keydown", function (e) {
      if (e.key !== "Tab") return;
      var stops = [input].concat($$("a", list)), at = stops.indexOf(document.activeElement);
      if (e.shiftKey && at <= 0) { e.preventDefault(); stops[stops.length - 1].focus(); }
      else if (!e.shiftKey && at === stops.length - 1) { e.preventDefault(); input.focus(); }
    });
    input.addEventListener("input", draw);
    input.addEventListener("keydown", function (e) {
      if (e.key === "ArrowDown") { active = Math.min(active + 1, results.length - 1); mark(); e.preventDefault(); }
      else if (e.key === "ArrowUp") { active = Math.max(active - 1, 0); mark(); e.preventDefault(); }
      else if (e.key === "Enter" && results[active]) { e.preventDefault(); var url = results[active].url; close(); location.href = url; }
    });
  }
  function open() {
    if (!dialog) build();
    if (dialog.open) return;
    opener = document.activeElement;
    input.value = "";
    dialog.showModal();
    input.focus();
    draw();
    loadIndex(function () { if (dialog.open) draw(); });
  }
  var trigger = document.querySelector("[data-search]");
  if (trigger) {
    trigger.addEventListener("click", open);
    /* Start fetching the index as soon as a reader reaches for search. */
    ["pointerenter", "focus"].forEach(function (t) { trigger.addEventListener(t, function () { loadIndex(function () {}); }, { once: true }); });
  }
  document.addEventListener("keydown", function (e) {
    var t = e.target, typing = t && (/^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName) || t.isContentEditable);
    if ((e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "k") { e.preventDefault(); open(); }
    else if (e.key === "/" && !typing && !(dialog && dialog.open)) { e.preventDefault(); open(); }
  });
})();
