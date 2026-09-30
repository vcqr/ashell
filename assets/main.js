/* ============================================================
   AShell 官网脚本 — 双语切换 / 功能卡片渲染 / 交互
   ============================================================ */
(function () {
  "use strict";

  /* ---------- 双语字典 ---------- */
  var I18N = {
    zh: {
      "nav.features": "功能特性",
      "nav.webserver": "Web 服务器",
      "nav.download": "下载",
      "nav.build": "源码构建",

      "hero.badge": "开源跨平台终端 · MIT License",
      "hero.tagline": "一个现代化、跨平台的终端 / SSH 客户端 —— 把本地终端、远程 SSH、SFTP、主机监控、端口转发与 AI 助手整合进一个桌面应用。",
      "hero.sub": "注重性能、隐私与可定制。凭证本地加密存储，AI 助手运行在你自己的机器上，密钥永不离开你的设备。",
      "hero.cta.download": "下载最新版",
      "hero.cta.github": "GitHub 仓库",
      "hero.cta.docker": "Docker 部署",

      "stats.platforms": "目标平台",
      "stats.modules": "功能模块",
      "stats.langs": "界面语言",
      "stats.forward": "端口转发模式",
      "stats.local": "数据留在本机",

      "pillars.title": "为什么选择 AShell",
      "p1.title": "性能",
      "p1.desc": "WebGL 硬件加速渲染，原生 ConPTY / forkpty，SFTP 流式传输——大输出、大文件依然行云流水。",
      "p2.title": "隐私",
      "p2.desc": "密码与私钥使用 AES-256-GCM 加密落盘，永不通过接口外泄；AI 密钥留在本机，不经任何第三方中转。",
      "p3.title": "可定制",
      "p3.desc": "主题、字体、透明度、壁纸、鼠标行为随你调配；中英双语界面，随选随切。",

      "features.title": "功能特性",
      "features.sub": "从连接到运维，一个应用全覆盖。",

      "web.title": "不只是桌面应用 —— Web 服务器模式",
      "web.desc": "同一套核心，Docker 一键部署。浏览器即可使用 SSH / SFTP / 终端与 AI 助手，数据全部留在你自己的服务器上。",
      "web.copy": "复制",
      "web.copied": "已复制 ✓",
      "web.recommend": "推荐",
      "web.opt1.t": "Docker 部署",
      "web.opt1.d": "拉取 GHCR 多架构镜像（amd64 / arm64），首次启动自动生成访问令牌。",
      "web.opt2.t": "本地构建镜像",
      "web.opt2.d": "交叉编译 musl 静态二进制，组装极简 alpine 镜像，容器内零编译。",
      "web.opt3.t": "直接运行二进制",
      "web.opt3.d": "构建前端产物后编译 ashell-server，一行命令在本机 8090 端口起服务。",

      "dl.title": "下载 AShell",
      "dl.desc": "前往 GitHub Releases 获取对应平台的安装包，所有平台均提供 x86_64 与 ARM64 构建。",
      "dl.cta": "前往 Releases",

      "build.title": "从源码构建",
      "build.desc": "需要 Node.js 18+、Rust stable 与 Bun（编译 AI sidecar）。开发模式支持 Vite + Rust 热重载，生产构建自动编译 sidecar 与前端。",
      "build.more": "查看完整文档 →",

      "footer.desc": "现代化跨平台终端 / SSH 客户端",
      "footer.made": "以 Tauri 2 + Vue 3 构建"
    },
    en: {
      "nav.features": "Features",
      "nav.webserver": "Web Server",
      "nav.download": "Download",
      "nav.build": "Build",

      "hero.badge": "Open-source cross-platform terminal · MIT License",
      "hero.tagline": "A modern, cross-platform terminal / SSH client — bundling a local terminal, remote SSH, SFTP, host monitoring, port forwarding, and an AI assistant into a single desktop app.",
      "hero.sub": "Built for performance, privacy, and customizability. Credentials are encrypted locally; the AI assistant runs on your own machine — your keys never leave your device.",
      "hero.cta.download": "Download Latest",
      "hero.cta.github": "GitHub Repo",
      "hero.cta.docker": "Docker Deploy",

      "stats.platforms": "Target platforms",
      "stats.modules": "Feature modules",
      "stats.langs": "UI languages",
      "stats.forward": "Forwarding modes",
      "stats.local": "Data stays local",

      "pillars.title": "Why AShell",
      "p1.title": "Performance",
      "p1.desc": "WebGL hardware-accelerated rendering, native ConPTY / forkpty, streaming SFTP — smooth even with huge output and large files.",
      "p2.title": "Privacy",
      "p2.desc": "Passwords and private keys are encrypted at rest with AES-256-GCM and never exposed; AI keys stay on your machine with no third-party relay.",
      "p3.title": "Customizability",
      "p3.desc": "Themes, fonts, opacity, wallpaper, mouse behavior — all yours to tune. Bilingual UI switches on demand.",

      "features.title": "Features",
      "features.sub": "From connectivity to operations, one app covers it all.",

      "web.title": "More than a desktop app — Web Server Mode",
      "web.desc": "Same core, one-command Docker deploy. Use SSH / SFTP / terminals and the AI assistant right in your browser, with all data on your own server.",
      "web.copy": "Copy",
      "web.copied": "Copied ✓",
      "web.recommend": "Recommended",
      "web.opt1.t": "Docker Deploy",
      "web.opt1.d": "Pull the multi-arch GHCR image (amd64 / arm64); an access token is generated on first start.",
      "web.opt2.t": "Build the Image Locally",
      "web.opt2.d": "Cross-compile musl static binaries and assemble a minimal alpine image — zero compilation inside the container.",
      "web.opt3.t": "Run the Binary Directly",
      "web.opt3.d": "Build the frontend, compile ashell-server, and serve on port 8090 with a single command.",

      "dl.title": "Download AShell",
      "dl.desc": "Grab the installer for your platform from GitHub Releases. Every platform ships x86_64 and ARM64 builds.",
      "dl.cta": "Go to Releases",

      "build.title": "Build from Source",
      "build.desc": "Requires Node.js 18+, Rust stable, and Bun (for the AI sidecar). Dev mode supports Vite + Rust hot reload; production builds compile the sidecar and frontend automatically.",
      "build.more": "Read the full docs →",

      "footer.desc": "A modern cross-platform terminal / SSH client",
      "footer.made": "Built with Tauri 2 + Vue 3"
    }
  };

  /* ---------- 功能卡片数据 ---------- */
  function svg(p) {
    return '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">' + p + "</svg>";
  }

  var FEATURES = [
    {
      svg: svg('<polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/>'),
      zh: { t: "终端", d: "多 Tab 多窗口，本地与远程一应俱全。", tags: ["ConPTY / forkpty", "WebGL 渲染", "Telnet / 串口", "进度条识别", "命令建议", "sudo 自动填充"] },
      en: { t: "Terminal", d: "Multi-tab, multi-window — local and remote alike.", tags: ["ConPTY / forkpty", "WebGL render", "Telnet / Serial", "Progress detection", "Suggestions", "sudo auto-fill"] }
    },
    {
      svg: svg('<rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="18" x2="6.01" y2="18"/>'),
      zh: { t: "主机管理", d: "无限层级目录树，凭证加密存储。", tags: ["拖拽排序", "AES-256-GCM", "图标 / 颜色标记", "分栏表单"] },
      en: { t: "Host Management", d: "Infinitely nested folder tree with encrypted credentials.", tags: ["Drag & drop", "AES-256-GCM", "Icon / color tags", "Split form"] }
    },
    {
      svg: svg('<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/>'),
      zh: { t: "SFTP 文件管理", d: "与终端共享同一 SSH 连接的文件管理器。", tags: ["流式上传 / 下载", "在线编辑", "面包屑导航", "权限着色"] },
      en: { t: "SFTP Files", d: "A file manager sharing the terminal's SSH connection.", tags: ["Streaming transfer", "Inline editor", "Breadcrumb path", "Permission coloring"] }
    },
    {
      svg: svg('<polyline points="22 12 18 12 15 21 9 3 6 12 2 12"/>'),
      zh: { t: "主机监控", d: "实时掌握远程主机资源与网络流量。", tags: ["CPU / 内存 / 磁盘", "Top 5 进程", "网络流量图", "1.5s 轮询"] },
      en: { t: "Monitoring", d: "Live view of remote resources and network traffic.", tags: ["CPU / Mem / Disk", "Top 5 processes", "Throughput chart", "1.5s polling"] }
    },
    {
      svg: svg('<path d="m17 2 4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="m7 22-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>'),
      zh: { t: "端口转发", d: "表单化创建转发规则，无需手敲命令。", tags: ["-L / -R / -D", "可视化规则表", "流量状态刷新"] },
      en: { t: "Port Forwarding", d: "Create forwarding rules from a form — no CLI needed.", tags: ["-L / -R / -D", "Visual rule table", "Live status"] }
    },
    {
      svg: svg('<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9"/><path d="M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5"/><circle cx="12" cy="12" r="2"/><path d="M16.2 7.8c2.3 2.3 2.3 6.1 0 8.5"/><path d="M19.1 4.9C23 8.8 23 15.2 19.1 19.1"/>'),
      zh: { t: "广播输入", d: "一次输入，多个终端按键级同步执行。", tags: ["按键级转发", "跨窗口", "目标可选"] },
      en: { t: "Broadcast Input", d: "Type once, synced keystroke-level across terminals.", tags: ["Keystroke relay", "Cross-window", "Selectable targets"] }
    },
    {
      svg: svg('<path d="M12 8V4H8"/><rect width="16" height="12" x="4" y="8" rx="2"/><path d="M2 14h2"/><path d="M20 14h2"/><circle cx="15" cy="13" r="1"/><circle cx="9" cy="13" r="1"/>'),
      zh: { t: "AI 助手", d: "内嵌 Claude Agent SDK 与 Pi 双引擎，按需选择。", tags: ["远程命令执行", "执行过程折叠", "破坏性操作确认", "自定义模型"] },
      en: { t: "AI Assistant", d: "Claude Agent SDK + Pi engines, factory-selected per session.", tags: ["Remote execution", "Foldable traces", "Confirm dangerous ops", "Custom models"] }
    },
    {
      svg: svg('<circle cx="13.5" cy="6.5" r=".6" fill="currentColor"/><circle cx="17.5" cy="10.5" r=".6" fill="currentColor"/><circle cx="8.5" cy="7.5" r=".6" fill="currentColor"/><circle cx="6.5" cy="12.5" r=".6" fill="currentColor"/><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c.9 0 1.5-.9 1.5-1.5 0-.4-.2-.7-.4-1-.3-.3-.4-.6-.4-1 0-.8.7-1.5 1.5-1.5H16c3.3 0 6-2.7 6-6 0-4.5-4.5-8-10-8Z"/>'),
      zh: { t: "窗口与外观", d: "毛玻璃、壁纸与透明度自由组合。", tags: ["Acrylic 模糊", "背景壁纸", "系统字体枚举", "暗 / 亮主题"] },
      en: { t: "Window & Appearance", d: "Blur, wallpaper, and opacity — freely combined.", tags: ["Acrylic blur", "Wallpaper", "Font enumeration", "Dark / Light"] }
    },
    {
      svg: svg('<circle cx="12" cy="12" r="10"/><path d="M2 12h20"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10Z"/>'),
      zh: { t: "国际化与启动", d: "中英双语全覆盖，会话恢复与自动连接。", tags: ["简体中文 / English", "会话恢复", "自动连接", "默认 Shell"] },
      en: { t: "i18n & Startup", d: "Full bilingual coverage, session restore and auto-connect.", tags: ["中文 / English", "Session restore", "Auto-connect", "Default shell"] }
    },
    {
      svg: svg('<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M3 21v-5h5"/>'),
      zh: { t: "自动更新", d: "新版本自动发现、一键下载并重启升级。", tags: ["启动检查", "一键安装", "Release Notes"] },
      en: { t: "Auto Update", d: "Detects new versions, one-click install and restart.", tags: ["Startup check", "One-click install", "Release Notes"] }
    }
  ];

  var lang = "zh";
  try {
    var saved = localStorage.getItem("ashell-lang");
    if (saved === "en" || saved === "zh") lang = saved;
    else if ((navigator.language || "").toLowerCase().indexOf("zh") !== 0) lang = "en";
  } catch (e) { /* localStorage 不可用时保持默认 */ }

  /* ---------- 应用语言 ---------- */
  function applyLang(next) {
    lang = next;
    var dict = I18N[lang];

    document.documentElement.lang = lang === "zh" ? "zh-CN" : "en";

    var nodes = document.querySelectorAll("[data-i18n]");
    for (var i = 0; i < nodes.length; i++) {
      var key = nodes[i].getAttribute("data-i18n");
      if (dict[key] != null) nodes[i].textContent = dict[key];
    }

    renderFeatures();
    updateToggle();

    try { localStorage.setItem("ashell-lang", lang); } catch (e) { }
  }

  /* ---------- 渲染功能卡片 ---------- */
  function renderFeatures() {
    var grid = document.getElementById("featuresGrid");
    if (!grid) return;
    var html = "";
    for (var i = 0; i < FEATURES.length; i++) {
      var f = FEATURES[i];
      var c = f[lang];
      html += '<article class="fcard reveal is-visible">'
        + '<div class="fcard__head"><div class="fcard__icon" aria-hidden="true">' + f.svg + '</div><h3>' + c.t + "</h3></div>"
        + "<p>" + c.d + "</p>"
        + '<div class="fcard__tags">';
      for (var j = 0; j < c.tags.length; j++) html += "<span>" + c.tags[j] + "</span>";
      html += "</div></article>";
    }
    grid.innerHTML = html;
  }

  /* ---------- 语言切换按钮 ---------- */
  var langToggle = document.getElementById("langToggle");
  var langLabel = document.getElementById("langLabel");
  var langThumb = document.getElementById("langThumb");

  function updateToggle() {
    if (!langToggle) return;
    // 按钮上显示「另一种语言」供切换
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

  /* ---------- 复制按钮 ---------- */
  document.addEventListener("click", function (e) {
    var btn = e.target.closest ? e.target.closest(".copy-btn") : null;
    if (!btn) return;
    var target = document.getElementById(btn.getAttribute("data-copy-target"));
    if (!target) return;
    var text = target.innerText;

    function done() {
      var dict = I18N[lang];
      btn.textContent = dict["web.copied"];
      btn.classList.add("is-copied");
      setTimeout(function () {
        btn.textContent = dict["web.copy"];
        btn.classList.remove("is-copied");
      }, 1800);
    }

    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(done, function () { fallbackCopy(text, done); });
    } else {
      fallbackCopy(text, done);
    }
  });

  function fallbackCopy(text, done) {
    var ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    try { document.execCommand("copy"); } catch (e) { }
    document.body.removeChild(ta);
    done();
  }

  /* ---------- 滚动入场动画 ---------- */
  var reduced = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  var revealEls = Array.prototype.slice.call(document.querySelectorAll(".reveal"));

  function markVisible(el) { el.classList.add("is-visible"); }

  if (reduced || !("IntersectionObserver" in window)) {
    revealEls.forEach(markVisible);
  } else {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (entry.isIntersecting) {
          markVisible(entry.target);
          io.unobserve(entry.target);
        }
      });
    }, { threshold: 0.12, rootMargin: "0px 0px -40px 0px" });
    revealEls.forEach(function (el) { io.observe(el); });
  }

  /* ---------- 年份 ---------- */
  var yearEl = document.getElementById("year");
  if (yearEl) yearEl.textContent = new Date().getFullYear();

  /* ---------- 初始化 ---------- */
  applyLang(lang);
})();
