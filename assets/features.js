/* ============================================================
   AShell 功能介绍页脚本 — 双语 / 分节渲染 / 目录高亮
   ============================================================ */
(function () {
  "use strict";

  /* ---------- 双语字典 ---------- */
  var I18N = {
    zh: {
      "nav.features": "功能介绍",
      "nav.webserver": "Web 服务器",
      "nav.download": "下载",
      "nav.build": "源码构建",
      "footer.desc": "现代化跨平台终端 / SSH 客户端",
      "footer.made": "以 Tauri 2 + Vue 3 构建",
      "doc.title": "功能介绍",
      "doc.sub":
        "从本地终端到远程运维，从文件传输到 AI 助手——逐项看 AShell 能做什么。",
      "doc.toc": "目录",
      "ph.title": "功能截图占位",
      "ph.note": "16:9 · 建议 1280×720，后续用真实截图替换",
    },
    en: {
      "nav.features": "Features",
      "nav.webserver": "Web Server",
      "nav.download": "Download",
      "nav.build": "Build",
      "footer.desc": "A modern cross-platform terminal / SSH client",
      "footer.made": "Built with Tauri 2 + Vue 3",
      "doc.title": "Features",
      "doc.sub":
        "From local terminals to remote ops, from file transfer to the AI assistant — see what AShell does, feature by feature.",
      "doc.toc": "Contents",
      "ph.title": "Screenshot placeholder",
      "ph.note": "16:9 · 1280×720 recommended, swap in a real screenshot later",
    },
  };

  /* ---------- 图标（与首页一致） ---------- */
  function svg(p) {
    return (
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' +
      p +
      "</svg>"
    );
  }

  /* ---------- 功能数据（含分节说明 detail） ---------- */
  var FEATURES = [
    {
      key: "terminal",
      svg: svg(
        '<polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/>',
      ),
      zh: {
        t: "终端",
        d: "多 Tab 多窗口，本地与远程一应俱全。",
        detail:
          "支持多标签页与多窗口，本地 shell 与远程 SSH 会话统一管理；WebGL 渲染保证大段输出流畅滚动，并能识别进度条、给出命令建议，sudo 提权时自动填充密码。",
        tags: [
          "ConPTY / forkpty",
          "WebGL 渲染",
          "Telnet / 串口",
          "进度条识别",
          "命令建议",
          "sudo 自动填充",
        ],
      },
      en: {
        t: "Terminal",
        d: "Multi-tab, multi-window — local and remote alike.",
        detail:
          "Multiple tabs and windows unify local shells and remote SSH sessions. WebGL rendering keeps long output scrolling smoothly, detects progress bars, suggests commands, and auto-fills sudo passwords on privilege escalation.",
        tags: [
          "ConPTY / forkpty",
          "WebGL render",
          "Telnet / Serial",
          "Progress detection",
          "Suggestions",
          "sudo auto-fill",
        ],
      },
    },
    {
      key: "host",
      svg: svg(
        '<rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/>',
      ),
      zh: {
        t: "主机管理",
        d: "无限层级目录树，凭证加密存储。",
        detail:
          "以无限层级的目录树组织主机与分组，支持拖拽排序；密码与私钥使用 AES-256-GCM 加密存储，可为每个主机设置图标与颜色标记，分栏表单让录入更顺手。",
        tags: ["拖拽排序", "AES-256-GCM", "图标 / 颜色标记", "分栏表单"],
      },
      en: {
        t: "Host Management",
        d: "Infinitely nested folder tree with encrypted credentials.",
        detail:
          "Organize hosts and groups in an infinitely nested tree with drag-and-drop sorting. Passwords and private keys are encrypted with AES-256-GCM; per-host icons, color tags, and a split form make entry painless.",
        tags: ["Drag & drop", "AES-256-GCM", "Icon / color tags", "Split form"],
      },
    },
    {
      key: "sftp",
      svg: svg(
        '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>',
      ),
      zh: {
        t: "SFTP 文件管理",
        d: "与终端共享同一 SSH 连接的文件管理器。",
        detail:
          "文件管理器复用终端已建立的 SSH 连接，无需重复登录；支持流式上传下载、在线编辑、面包屑导航，并按 Unix 权限着色文件，传输与命令互不干扰。",
        tags: ["流式上传 / 下载", "在线编辑", "面包屑导航", "权限着色"],
      },
      en: {
        t: "SFTP Files",
        d: "A file manager sharing the terminal's SSH connection.",
        detail:
          "The file manager reuses the terminal's live SSH connection — no second login. Streaming transfer, inline editing, breadcrumb paths, and permission-based coloring keep transfers and commands independent.",
        tags: [
          "Streaming transfer",
          "Inline editor",
          "Breadcrumb path",
          "Permission coloring",
        ],
      },
    },
    {
      key: "monitor",
      svg: svg('<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>'),
      zh: {
        t: "主机监控",
        d: "实时掌握远程主机资源与网络流量。",
        detail:
          "实时监控远程主机的 CPU、内存、磁盘与网络流量，列出资源占用最高的 5 个进程，每 1.5 秒轮询刷新，问题主机一眼可见。",
        tags: ["CPU / 内存 / 磁盘", "Top 5 进程", "网络流量图", "1.5s 轮询"],
      },
      en: {
        t: "Monitoring",
        d: "Live view of remote resources and network traffic.",
        detail:
          "Watch remote CPU, memory, disk, and network throughput in real time, with the top 5 processes by usage and a 1.5-second polling interval — problem hosts stand out at a glance.",
        tags: [
          "CPU / Mem / Disk",
          "Top 5 processes",
          "Throughput chart",
          "1.5s polling",
        ],
      },
    },
    {
      key: "forward",
      svg: svg(
        '<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>',
      ),
      zh: {
        t: "端口转发",
        d: "表单化创建转发规则，无需手敲命令。",
        detail:
          "通过表单创建本地(-L)、远程(-R)与动态(-D)转发规则，规则以表格可视化呈现，并可实时刷新每条规则的流量状态，告别手敲 ssh -L。",
        tags: ["-L / -R / -D", "可视化规则表", "流量状态刷新"],
      },
      en: {
        t: "Port Forwarding",
        d: "Create forwarding rules from a form — no CLI needed.",
        detail:
          "Build local (-L), remote (-R), and dynamic (-D) rules from a form. Rules show in a visual table with live per-rule traffic status — no more hand-typed ssh -L.",
        tags: ["-L / -R / -D", "Visual rule table", "Live status"],
      },
    },
    {
      key: "broadcast",
      svg: svg(
        '<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9"/><path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"/><circle cx="12" cy="12" r="2"/><path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"/><path d="M19.1 4.9C23 8.8 23 15.2 19.1 19.1"/>',
      ),
      zh: {
        t: "广播输入",
        d: "一次输入，多个终端按键级同步执行。",
        detail:
          "在一个终端输入，按键级同步转发到多个目标终端，可跨窗口选择接收目标，适合批量执行同一条命令的运维场景。",
        tags: ["按键级转发", "跨窗口", "目标可选"],
      },
      en: {
        t: "Broadcast Input",
        d: "Type once, synced keystroke-level across terminals.",
        detail:
          "Type in one terminal and relay keystrokes to many targets with cross-window selection — ideal for running the same command across a fleet.",
        tags: ["Keystroke relay", "Cross-window", "Selectable targets"],
      },
    },
    {
      key: "ai",
      svg: svg(
        '<path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><circle cx="15" cy="13" r="1"/><circle cx="9" cy="13" r="1"/>',
      ),
      zh: {
        t: "AI 助手",
        d: "内嵌 Claude Agent SDK 与 Pi 双引擎，按需选择。",
        detail:
          "内嵌 Claude Agent SDK 与 Pi 双引擎，可按会话选择；支持远程命令执行、执行过程折叠展示，破坏性操作需手动确认，并支持自定义模型接入。",
        tags: ["远程命令执行", "执行过程折叠", "破坏性操作确认", "自定义模型"],
      },
      en: {
        t: "AI Assistant",
        d: "Claude Agent SDK + Pi engines, factory-selected per session.",
        detail:
          "Embeds both Claude Agent SDK and Pi engines, selectable per session. Remote command execution, foldable traces, confirmation for destructive actions, and custom model support.",
        tags: [
          "Remote execution",
          "Foldable traces",
          "Confirm dangerous ops",
          "Custom models",
        ],
      },
    },
    {
      key: "appearance",
      svg: svg(
        '<circle cx="13.5" cy="6.5" r=".6" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".6" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".6" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".6" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.5-.9 1.5-1.5 0-.4-.2-.7-.4-1-.3-.3-.4-.6-.4-1 0-.8.7-1.5 1.5-1.5H16c3.3 0 6-2.7 6-6 0-4.5-4.5-8-10-8Z"/>',
      ),
      zh: {
        t: "窗口与外观",
        d: "毛玻璃、壁纸与透明度自由组合。",
        detail:
          "提供 Acrylic 毛玻璃模糊、背景壁纸与窗口透明度调节，自动枚举系统已安装字体，并支持暗色与亮色主题，外观随手可调。",
        tags: ["Acrylic 模糊", "背景壁纸", "系统字体枚举", "暗 / 亮主题"],
      },
      en: {
        t: "Window & Appearance",
        d: "Blur, wallpaper, and opacity — freely combined.",
        detail:
          "Acrylic blur, wallpaper, and window opacity controls, automatic system font enumeration, plus dark and light themes — tune the look however you like.",
        tags: ["Acrylic blur", "Wallpaper", "Font enumeration", "Dark / Light"],
      },
    },
    {
      key: "i18n",
      svg: svg(
        '<circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z"/>',
      ),
      zh: {
        t: "国际化与启动",
        d: "中英双语全覆盖，会话恢复与自动连接。",
        detail:
          "界面完整中英双语；启动时自动恢复上次会话布局，可按配置自动连接指定主机，并设置默认 Shell，开箱即用。",
        tags: ["简体中文 / English", "会话恢复", "自动连接", "默认 Shell"],
      },
      en: {
        t: "i18n & Startup",
        d: "Full bilingual coverage, session restore and auto-connect.",
        detail:
          "Complete Chinese / English UI. Restore your last session layout on launch, auto-connect to configured hosts, and set a default shell — ready to go.",
        tags: [
          "中文 / English",
          "Session restore",
          "Auto-connect",
          "Default shell",
        ],
      },
    },
    {
      key: "update",
      svg: svg(
        '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M3 21v-5h5"/>',
      ),
      zh: {
        t: "自动更新",
        d: "新版本自动发现、一键下载并重启升级。",
        detail:
          "启动时检查新版本，发现后一键下载并重启完成升级，同时展示 Release Notes，让你清楚每次更新改了什么。",
        tags: ["启动检查", "一键安装", "Release Notes"],
      },
      en: {
        t: "Auto Update",
        d: "Detects new versions, one-click install and restart.",
        detail:
          "Checks for new versions on launch; one click downloads, restarts, and upgrades — with Release Notes shown so you know exactly what changed.",
        tags: ["Startup check", "One-click install", "Release Notes"],
      },
    },
  ];

  var lang = "zh";
  try {
    var saved = localStorage.getItem("ashell-lang");
    if (saved === "en" || saved === "zh") lang = saved;
    else if ((navigator.language || "").toLowerCase().indexOf("zh") !== 0)
      lang = "en";
  } catch (e) {
    /* localStorage 不可用时保持默认 */
  }

  var PH_ICON = svg(
    '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/>',
  );

  function applyLang(next) {
    lang = next;
    var dict = I18N[lang];
    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";

    var nodes = document.querySelectorAll("[data-i18n]");
    for (var i = 0; i < nodes.length; i++) {
      var key = nodes[i].getAttribute("data-i18n");
      if (dict[key] != null) nodes[i].textContent = dict[key];
    }

    renderFeatures(dict);
    updateToggle();
    try {
      localStorage.setItem("ashell-lang", lang);
    } catch (e) {}
  }

  /* ---------- 功能截图 ----------
     把图片放进 assets/ 下，文件名按 key 命名，再在下方对象里填一行即可自动替换占位图。
     key 列表：terminal / host / sftp / monitor / forward / broadcast / ai / appearance / i18n / update
     例：terminal: "assets/feat-terminal.png", */
  var SHOTS = {
    terminal: "assets/ui/feat-terminal.png",
    host: "assets/ui/feat-host.png",
    sftp: "assets/ui/feat-sftp.png",
    monitor: "assets/ui/feat-monitor.png",
    forward: "assets/ui/feat-forward.png",
    broadcast: "assets/ui/feat-broadcast.png",
    ai: "assets/ui/feat-ai.png",
    appearance: "assets/ui/feat-appearance.png",
    i18n: "assets/ui/feat-i18n.png",
    update: "assets/ui/feat-update.png",
  };

  function renderFeatures(dict) {
    var toc = document.getElementById("docToc");
    var content = document.getElementById("docContent");
    if (!toc || !content) return;

    var tocHtml = "";
    var html = "";
    for (var i = 0; i < FEATURES.length; i++) {
      var f = FEATURES[i];
      var c = f[lang];
      var num = String(i + 1).padStart(2, "0");
      var id = "feat-" + f.key;

      tocHtml +=
        '<li><a href="#' +
        id +
        '" data-toc="' +
        id +
        '"><span class="doc__toc-num">' +
        num +
        "</span> · " +
        c.t +
        "</a></li>";

      html +=
        '<section class="feat-section reveal" id="' +
        id +
        '">' +
        '<div class="feat-section__head">' +
        '<div class="feat-section__icon" aria-hidden="true">' +
        f.svg +
        "</div>" +
        "<h2>" +
        c.t +
        "</h2>" +
        "</div>" +
        '<p class="feat-section__desc">' +
        c.detail +
        "</p>" +
        '<div class="feat-section__tags">';
      for (var j = 0; j < c.tags.length; j++)
        html += "<span>" + c.tags[j] + "</span>";
      html += "</div>";
      var shot = SHOTS[f.key];
      if (shot) {
        html +=
          '<div class="shot feat-shot"><img src="' +
          shot +
          '" alt="' +
          c.t +
          '" loading="lazy" /></div>';
      } else {
        html +=
          '<div class="shot-placeholder">' +
          '<div class="shot-placeholder__inner">' +
          PH_ICON +
          "<strong>" +
          dict["ph.title"] +
          "</strong>" +
          "<span>" +
          dict["ph.note"] +
          "</span>" +
          "</div>" +
          '<!-- 替换为截图：<img src="assets/feat-' +
          f.key +
          '.png" alt="' +
          c.t +
          '"> -->' +
          "</div>";
      }
      html += "</section>";
    }
    toc.innerHTML = tocHtml;
    content.innerHTML = html;

    observeReveal();
    setupSpy();
  }

  /* ---------- 语言切换按钮 ---------- */
  var langToggle = document.getElementById("langToggle");
  var langLabel = document.getElementById("langLabel");
  var langThumb = document.getElementById("langThumb");
  function updateToggle() {
    if (!langToggle) return;
    langLabel.textContent = lang === "zh" ? "EN" : "中";
    langThumb.textContent = lang === "zh" ? "中" : "E";
    langToggle.classList.toggle("is-en", lang === "en");
    langToggle.classList.toggle("is-zh", lang === "zh");
  }
  if (langToggle) {
    langToggle.addEventListener("click", function () {
      applyLang(lang === "zh" ? "en" : "zh");
    });
  }

  /* ---------- 汉堡菜单 ---------- */
  var burger = document.getElementById("navBurger");
  var navLinks = document.getElementById("navLinks");
  if (burger && navLinks) {
    burger.addEventListener("click", function () {
      var open = navLinks.classList.toggle("is-open");
      burger.classList.toggle("is-open", open);
      burger.setAttribute("aria-expanded", open ? "true" : "false");
    });
    navLinks.addEventListener("click", function (e) {
      if (e.target.tagName === "A") {
        navLinks.classList.remove("is-open");
        burger.classList.remove("is-open");
        burger.setAttribute("aria-expanded", "false");
      }
    });
  }

  /* ---------- 导航滚动态 ---------- */
  var nav = document.getElementById("nav");
  function onScroll() {
    if (nav) nav.classList.toggle("is-scrolled", window.scrollY > 8);
  }
  window.addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  /* ---------- 目录高亮（scroll-spy） ---------- */
  var spy = null;
  function setupSpy() {
    if (spy) spy.disconnect();
    var links = document.querySelectorAll("[data-toc]");
    if (!("IntersectionObserver" in window)) return;
    var map = {};
    links.forEach(function (a) {
      map[a.getAttribute("data-toc")] = a;
    });
    spy = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            links.forEach(function (a) {
              a.classList.remove("is-active");
            });
            var active = map[entry.target.id];
            if (active) active.classList.add("is-active");
          }
        });
      },
      { rootMargin: "-20% 0px -70% 0px", threshold: 0 },
    );
    document.querySelectorAll(".feat-section").forEach(function (s) {
      spy.observe(s);
    });
  }

  /* ---------- 滚动入场动画 ---------- */
  var revealObserver = null;
  function observeReveal() {
    if (revealObserver) revealObserver.disconnect();
    var els = Array.prototype.slice.call(document.querySelectorAll(".reveal"));
    var reduced =
      window.matchMedia &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduced || !("IntersectionObserver" in window)) {
      els.forEach(function (el) {
        el.classList.add("is-visible");
      });
      return;
    }
    revealObserver = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            revealObserver.unobserve(entry.target);
          }
        });
      },
      { threshold: 0.1, rootMargin: "0px 0px -40px 0px" },
    );
    els.forEach(function (el) {
      revealObserver.observe(el);
    });
  }

  /* ---------- 年份 ---------- */
  var yearEl = document.getElementById("year");
  if (yearEl) yearEl.textContent = new Date().getFullYear();

  /* ---------- 初始化 ---------- */
  applyLang(lang);
})();
