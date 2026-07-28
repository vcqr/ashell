/**
 * 常见命令字典：命令名 -> 双语简述。
 *
 * useCommandSuggest 的 COMMON_COMMANDS 由 Object.keys(COMMAND_DICT) 派生；
 * 历史命令在字典中找不到描述时，前端只显示命令名不显示描述列。
 */
export interface CommandDesc {
  zh: string
  en: string
}

export const COMMAND_DICT: Record<string, CommandDesc> = {
  // ---- 文件 / 目录 ----
  ls: { zh: "列出目录内容", en: "List directory contents" },
  cd: { zh: "切换当前目录", en: "Change directory" },
  pwd: { zh: "显示当前目录路径", en: "Print working directory" },
  mkdir: { zh: "创建目录", en: "Create directory" },
  rm: { zh: "删除文件或目录", en: "Remove files or directories" },
  cp: { zh: "复制文件或目录", en: "Copy files or directories" },
  mv: { zh: "移动或重命名文件", en: "Move or rename files" },
  cat: { zh: "查看文件内容", en: "Display file contents" },
  less: { zh: "分页查看文件", en: "View file paginated" },
  head: { zh: "显示文件开头若干行", en: "Show first lines of file" },
  tail: { zh: "显示文件末尾若干行", en: "Show last lines of file" },
  touch: { zh: "创建空文件或更新时间戳", en: "Create empty file / update timestamp" },
  ln: { zh: "创建链接", en: "Create links" },
  find: { zh: "搜索文件", en: "Search for files" },
  tree: { zh: "树状显示目录", en: "Display directory tree" },
  file: { zh: "识别文件类型", en: "Identify file type" },
  stat: { zh: "显示文件状态", en: "Display file status" },
  dd: { zh: "按块复制转换数据", en: "Convert and copy data" },

  // ---- 文本处理 ----
  grep: { zh: "文本搜索", en: "Search text patterns" },
  awk: { zh: "文本处理语言", en: "Text processing language" },
  sed: { zh: "流编辑器", en: "Stream editor" },
  sort: { zh: "排序文本行", en: "Sort lines of text" },
  uniq: { zh: "去除重复行", en: "Remove duplicate lines" },
  wc: { zh: "统计行/词/字节", en: "Count lines/words/bytes" },
  diff: { zh: "比较文件差异", en: "Compare files" },
  cut: { zh: "按列截取文本", en: "Cut text by columns" },
  tr: { zh: "替换或删除字符", en: "Translate/delete characters" },
  xargs: { zh: "构建并执行命令行", en: "Build and execute command lines" },
  tee: { zh: "输出到文件和标准输出", en: "Output to file and stdout" },
  jq: { zh: "JSON 处理工具", en: "JSON processor" },

  // ---- 权限 / 用户 ----
  chmod: { zh: "修改文件权限", en: "Change file permissions" },
  chown: { zh: "修改文件所有者", en: "Change file owner" },
  sudo: { zh: "以管理员权限执行", en: "Execute as superuser" },
  su: { zh: "切换用户", en: "Switch user" },
  passwd: { zh: "修改密码", en: "Change password" },
  useradd: { zh: "添加用户", en: "Add user" },
  usermod: { zh: "修改用户属性", en: "Modify user account" },
  whoami: { zh: "显示当前用户", en: "Show current user" },
  id: { zh: "显示用户与组 ID", en: "Show user and group IDs" },
  groups: { zh: "显示用户所属组", en: "Show group membership" },
  chgrp: { zh: "修改文件所属组", en: "Change file group" },

  // ---- 网络 ----
  ssh: { zh: "远程登录", en: "Remote login" },
  scp: { zh: "安全复制文件", en: "Secure copy" },
  rsync: { zh: "增量同步文件", en: "Incremental file sync" },
  curl: { zh: "HTTP 请求工具", en: "HTTP request tool" },
  wget: { zh: "下载文件", en: "Download files" },
  ping: { zh: "测试网络连通性", en: "Test network connectivity" },
  ifconfig: { zh: "查看/配置网卡", en: "View/configure network interfaces" },
  ip: { zh: "网络配置", en: "Network configuration" },
  netstat: { zh: "查看网络状态", en: "Show network status" },
  ss: { zh: "查看套接字状态", en: "Show socket statistics" },
  traceroute: { zh: "路由追踪", en: "Trace network route" },
  dig: { zh: "DNS 查询", en: "DNS lookup" },
  nslookup: { zh: "DNS 查询", en: "DNS lookup" },
  sftp: { zh: "安全文件传输", en: "Secure file transfer" },
  nc: { zh: "网络调试工具", en: "Networking utility (netcat)" },
  telnet: { zh: "远程登录（明文）", en: "Remote login (plaintext)" },

  // ---- 进程 / 系统 ----
  ps: { zh: "查看进程", en: "Show processes" },
  top: { zh: "实时进程监控", en: "Process monitor" },
  htop: { zh: "交互式进程监控", en: "Interactive process viewer" },
  kill: { zh: "终止进程", en: "Terminate process" },
  killall: { zh: "按名称终止进程", en: "Kill process by name" },
  df: { zh: "查看磁盘空间", en: "Show disk space" },
  du: { zh: "查看目录大小", en: "Show directory size" },
  free: { zh: "查看内存使用", en: "Show memory usage" },
  mount: { zh: "挂载文件系统", en: "Mount filesystem" },
  umount: { zh: "卸载文件系统", en: "Unmount filesystem" },
  uname: { zh: "显示系统信息", en: "Show system info" },
  uptime: { zh: "查看运行时间与负载", en: "Show uptime and load" },
  env: { zh: "查看环境变量", en: "Show environment variables" },
  which: { zh: "查找命令路径", en: "Locate command path" },
  whereis: { zh: "查找命令及相关文件", en: "Locate command and files" },
  man: { zh: "查看命令手册", en: "View manual pages" },
  date: { zh: "显示或设置日期时间", en: "Show or set date/time" },
  hostname: { zh: "显示或设置主机名", en: "Show or set hostname" },
  lsof: { zh: "列出进程打开的文件", en: "List open files" },
  watch: { zh: "周期性执行命令", en: "Execute command periodically" },
  shutdown: { zh: "关机或重启", en: "Shutdown or restart" },
  reboot: { zh: "重启系统", en: "Reboot system" },
  lsblk: { zh: "列出块设备", en: "List block devices" },
  fdisk: { zh: "磁盘分区工具", en: "Disk partition tool" },

  // ---- 包管理 ----
  apt: { zh: "Debian/Ubuntu 包管理", en: "Debian package manager" },
  yum: { zh: "RHEL/CentOS 包管理", en: "RHEL package manager" },
  dnf: { zh: "Fedora 包管理", en: "Fedora package manager" },
  pacman: { zh: "Arch Linux 包管理", en: "Arch package manager" },
  brew: { zh: "macOS Homebrew", en: "macOS Homebrew" },
  snap: { zh: "Snap 包管理", en: "Snap package manager" },

  // ---- 开发 / 运维 ----
  git: { zh: "版本控制", en: "Version control" },
  docker: { zh: "容器引擎", en: "Container engine" },
  kubectl: { zh: "Kubernetes 管理", en: "Kubernetes management" },
  systemctl: { zh: "管理系统服务", en: "Manage system services" },
  journalctl: { zh: "查看系统日志", en: "View system logs" },
  crontab: { zh: "定时任务管理", en: "Manage cron jobs" },
  service: { zh: "管理系统服务（SysV）", en: "Manage services (SysV)" },

  // ---- 压缩 / 归档 ----
  tar: { zh: "归档工具", en: "Archive tool" },
  zip: { zh: "压缩为 zip", en: "Compress to zip" },
  unzip: { zh: "解压 zip", en: "Extract zip" },
  gzip: { zh: "gzip 压缩", en: "gzip compression" },
  gunzip: { zh: "gzip 解压", en: "gzip decompression" },

  // ---- 编辑器 ----
  vim: { zh: "Vim 编辑器", en: "Vim editor" },
  nano: { zh: "Nano 编辑器", en: "Nano editor" },
  emacs: { zh: "Emacs 编辑器", en: "Emacs editor" },

  // ---- Shell 内建 ----
  echo: { zh: "输出文本", en: "Print text" },
  printf: { zh: "格式化输出", en: "Formatted output" },
  export: { zh: "设置环境变量", en: "Set environment variable" },
  source: { zh: "在当前 Shell 执行脚本", en: "Execute script in current shell" },
  alias: { zh: "设置命令别名", en: "Set command alias" },
  history: { zh: "查看命令历史", en: "Show command history" },
  nohup: { zh: "忽略挂断信号运行", en: "Run immune to hangups" },
  clear: { zh: "清屏", en: "Clear screen" },
  exit: { zh: "退出当前 Shell", en: "Exit current shell" },
  logout: { zh: "注销登录会话", en: "Log out of session" },

  // ---- 终端复用 ----
  screen: { zh: "GNU Screen 终端复用器", en: "GNU Screen multiplexer" },
  tmux: { zh: "tmux 终端复用器", en: "tmux terminal multiplexer" },

  // ---- 编程语言 / 工具链 ----
  python: { zh: "Python 解释器", en: "Python interpreter" },
  python3: { zh: "Python 3 解释器", en: "Python 3 interpreter" },
  node: { zh: "Node.js 运行时", en: "Node.js runtime" },
  npm: { zh: "Node 包管理", en: "Node package manager" },
  npx: { zh: "执行 Node 包", en: "Execute Node package" },
  pnpm: { zh: "快速 Node 包管理", en: "Fast Node package manager" },
  yarn: { zh: "Node 包管理", en: "Node package manager" },
  pip: { zh: "Python 包管理", en: "Python package manager" },
  pip3: { zh: "Python 3 包管理", en: "Python 3 package manager" },
  cargo: { zh: "Rust 包管理", en: "Rust package manager" },
  rustc: { zh: "Rust 编译器", en: "Rust compiler" },
  go: { zh: "Go 语言工具", en: "Go language tool" },
  java: { zh: "Java 运行时", en: "Java runtime" },
  javac: { zh: "Java 编译器", en: "Java compiler" },
  mvn: { zh: "Maven 构建工具", en: "Maven build tool" },
  gradle: { zh: "Gradle 构建工具", en: "Gradle build tool" },
  make: { zh: "构建工具", en: "Build tool" },

  // ---- Windows 命令 ----
  dir: { zh: "列出目录内容", en: "List directory contents" },
  cls: { zh: "清屏", en: "Clear screen" },
  type: { zh: "显示文件内容", en: "Display file contents" },
  del: { zh: "删除文件", en: "Delete files" },
  copy: { zh: "复制文件", en: "Copy files" },
  move: { zh: "移动文件", en: "Move files" },
  ren: { zh: "重命名文件", en: "Rename files" },
  md: { zh: "创建目录", en: "Make directory" },
  rd: { zh: "删除目录", en: "Remove directory" },
  ipconfig: { zh: "网络配置", en: "Network configuration" },
  tasklist: { zh: "查看进程列表", en: "List processes" },
  taskkill: { zh: "终止进程", en: "Kill process" },
  net: { zh: "网络管理", en: "Network management" },
  sc: { zh: "服务管理", en: "Service control" },
  wmic: { zh: "WMI 命令行", en: "WMI command line" },
  powershell: { zh: "PowerShell", en: "PowerShell" },
  cmd: { zh: "命令提示符", en: "Command prompt" },
}

export type Locale = "zh-CN" | "en-US"

/** 包装命令：后接真正要执行的命令，取首词时跳过。 */
const WRAPPER_COMMANDS = new Set([
  "sudo",
  "doas",
  "nohup",
  "time",
  "command",
  "builtin",
  "exec",
  "stdbuf",
  "nice",
  "ionice",
  "env",
])

/**
 * 从一条完整命令中提取首词（命令名），大小写保持原样。
 * 跳过前导包装命令（sudo / nohup / time 等）、选项与 VAR=value 赋值：
 * "sudo apt update" -> "apt"，"FOO=bar git status" -> "git"。
 * 整条命令只有包装词本身时保留原词，保证 "sudo" 仍能查到自身描述。
 */
export function extractFirstWord(fullCmd: string): string {
  const words = fullCmd.trim().split(/\s+/)
  let i = 0
  while (i < words.length - 1) {
    const w = words[i]
    if (w === undefined) break
    if (WRAPPER_COMMANDS.has(w.toLowerCase()) || w.startsWith("-") || /^\w+=/.test(w)) {
      i++
    } else {
      break
    }
  }
  return words[i] ?? ""
}

/** 判断命令名是否为字典内置命令。大小写不敏感，不查原型链。 */
export function isBuiltinCommand(cmd: string): boolean {
  return Object.hasOwn(COMMAND_DICT, cmd.toLowerCase())
}

function lookupDesc(name: string): CommandDesc | undefined {
  const key = name.toLowerCase()
  return Object.hasOwn(COMMAND_DICT, key) ? COMMAND_DICT[key] : undefined
}

/** 获取命令描述（接受完整命令行，取首词查找），找不到时返回空串。大小写不敏感。 */
export function getCommandDesc(cmd: string, locale: Locale): string {
  const entry = lookupDesc(extractFirstWord(cmd))
  if (!entry) return ""
  return locale === "zh-CN" ? entry.zh : entry.en
}
