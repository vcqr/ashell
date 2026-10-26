/**
 * 危险命令静态检测（服务端判定，不依赖模型自觉）。
 *
 * 定位：防模型犯错/幻觉/被注入的护栏，不是对抗攻击者的安全边界。
 * 变量间接引用（CMD="rm -rf /"; $CMD）静态检测无法覆盖，属已知残余风险；
 * 通过"混淆模式"规则（管道进 shell、eval、xargs 执行）兜底大部分编码绕过。
 *
 * 两层结构：
 * - 结构规则：对整条命令匹配，抓"内容不可静态判定但模式本身高危"的执行方式
 *   （curl | sh 等）；必须在按分隔符拆段之前跑，否则管道被拆掉后无法命中。
 * - 内容规则：按 shell 分隔符（; && || | $( 反引号 换行）拆段后逐段匹配，
 *   危险动作藏在复合命令中间也能被抓到；规则锚定段首动词，避免
 *   `last reboot`、`grep passwd` 这类只读命令误伤。
 */

export interface DangerVerdict {
  dangerous: boolean;
  /** 命中的风险说明（中文，供审批卡片直接展示） */
  reason?: string;
  /** 命中规则的那段子命令，便于定位复合命令里的危险部分 */
  segment?: string;
}

interface ContentRule {
  re: RegExp;
  reason: string;
}

/** 结构规则：整串匹配，不拆段 */
const STRUCTURAL_RULES: ContentRule[] = [
  // 见模式即拦，不看管道内容——payload 编码绕不过"总要交给 shell 解释"这一步
  {
    re: /\|\s*(?:sudo\s+)?(?:ba|z|da|k|fi|a)?sh\b/,
    reason: "把管道内容直接交给 shell 执行",
  },
  {
    re: /\bxargs\s+(?:sudo\s+)?(?:ba|z|da|k|fi|a)?sh\b/,
    reason: "经 xargs 把内容交给 shell 执行",
  },
  {
    re: /\bxargs\s+(?:sudo\s+)?rm\b/,
    reason: "经 xargs 批量删除文件",
  },
];

/**
 * 内容规则：锚定段首动词（已剥离 sudo 与 VAR=value 前缀）。
 * 匹配发生在小写化 + 去引号 + 压缩空白之后。
 */
const CONTENT_RULES: ContentRule[] = [
  // ── 文件系统 ──
  { re: /^rm\b[^;]*\s(?:\/|~|\*)/, reason: "删除绝对路径/主目录/通配符目标 (rm)" },
  { re: /^rm\s+-\w*(?:r|f)\w*\s/, reason: "递归/强制删除 (rm -r/-f)" },
  { re: /^mkfs/, reason: "格式化文件系统 (mkfs)" },
  { re: /^wipefs\b/, reason: "擦除文件系统签名 (wipefs)" },
  { re: /^fdisk\b(?!.*\s-l\b)/, reason: "磁盘分区操作 (fdisk)" },
  { re: /^sfdisk\b/, reason: "磁盘分区操作 (sfdisk)" },
  { re: /^parted\b/, reason: "磁盘分区操作 (parted)" },
  { re: /^shred\b/, reason: "粉碎文件 (shred)" },
  { re: /^truncate\s+-s\s*0/, reason: "截断文件为零 (truncate -s 0)" },
  { re: /^find\b[^|]*-delete\b/, reason: "find 批量删除 (-delete)" },
  // 递归 777：chmod 在前、-R 类标志与 777 都要出现
  { re: /^chmod\b(?=[^;]*\s-[a-z]*r)(?=[^;]*\b777\b)/, reason: "递归开放全部权限 (chmod -R 777)" },

  // ── 电源/系统 ──
  { re: /^(?:shutdown|reboot|halt|poweroff)\b/, reason: "关机/重启系统" },
  { re: /^init\s+[06]\b/, reason: "切换系统运行级 (init 0/6)" },

  // ── 防火墙 ──
  { re: /^iptables6?\s+(?:.*\s)?(?:-f\b|--flush\b)/, reason: "清空防火墙规则 (iptables -F)" },
  { re: /^ufw\s+(?:disable|reset)\b/, reason: "关闭/重置防火墙 (ufw)" },
  { re: /^nft\s+flush\b/, reason: "清空 nftables 规则 (nft flush)" },

  // ── 用户/认证 ──
  { re: /^(?:userdel|groupdel|passwd|usermod|visudo)\b/, reason: "变更用户/认证配置" },
  { re: /\/etc\/(?:sudoers|shadow)\b/, reason: "修改敏感系统文件" },
  { re: /authorized_keys\b/, reason: "修改 SSH 授权密钥" },

  // ── 进程 ──
  { re: /^kill\s+-[a-z]*9[a-z]*\s+(?:1|%1)\b/, reason: "向 PID 1 发送 SIGKILL" },
  { re: /^(?:killall|pkill)\b/, reason: "按名批量杀进程" },

  // ── 数据/包/容器/编排 ──
  // 不锚定段首：mysql -e / psql -c 等 SQL 客户端调用里也会出现
  { re: /\bdrop\s+(?:database|table)\b/, reason: "删除数据库/表" },
  { re: /\btruncate\s+table\b/, reason: "清空数据表 (truncate table)" },
  { re: /^apt(?:-get)?\s+(?:remove|purge|autoremove)\b/, reason: "卸载系统软件包 (apt)" },
  { re: /^(?:yum|dnf)\s+(?:remove|erase)\b/, reason: "卸载系统软件包" },
  { re: /^docker\s+system\s+prune\b/, reason: "清理 Docker 全部未用资源" },
  { re: /^kubectl\s+delete\b/, reason: "删除 K8s 资源" },

  // ── 痕迹 ──
  { re: /^crontab\s+-r\b/, reason: "删除全部定时任务 (crontab -r)" },
  { re: /^history\s+-c\b/, reason: "清除命令历史" },

  // ── 混淆（锚定段首）──
  { re: /^eval\b/, reason: "eval 动态拼接执行" },
];

/**
 * 敏感目标规则：即使段首是 echo/printf（参数是数据不是命令）也要匹配——
 * `echo key >> ~/.ssh/authorized_keys` 是加公钥的标准写法，属于应审批的变更；
 * 任何命令重定向/写入块设备路径同样命中。
 */
const SENSITIVE_TARGET_RULES: ContentRule[] = [
  { re: /\/etc\/(?:sudoers|shadow)\b/, reason: "修改敏感系统文件" },
  { re: /authorized_keys\b/, reason: "修改 SSH 授权密钥" },
  // 只拦写入形态（of=/重定向），`df -h /dev/sda1` 这类只读提及不误伤
  { re: /(?:\bof=|>\s*)\/dev\/(?:sd|hd|vd|nvme|xvd|loop|disk|mapper)/, reason: "直接写入块设备" },
];

/** 单段归一化：去引号、小写、压缩空白 */
function normalizeSegment(seg: string): string {
  return seg
    .replace(/['"]/g, "")
    .toLowerCase()
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * 检测一条命令是否危险。
 * 非破坏性日常操作应返回 dangerous=false 直接放行；命中即要求人工 y/n。
 */
export function detectDanger(cmd: string): DangerVerdict {
  const joined = cmd.replace(/\\\r?\n/g, " ");

  // 结构规则先行：整串匹配（此时管道/分隔符都还在）
  const structural = joined.replace(/['"]/g, "").toLowerCase();
  for (const rule of STRUCTURAL_RULES) {
    const m = structural.match(rule.re);
    if (m) {
      return { dangerous: true, reason: rule.reason, segment: m[0] };
    }
  }

  // 拆段：; && || | & $() 反引号 换行 都视为分隔/包裹符
  const segments = joined
    .split(/[;|&\r\n]|\$\(|`/g)
    .map(normalizeSegment)
    .filter((s) => s.length > 0);

  for (const seg of segments) {
    // 敏感目标优先：echo/printf 的参数也要查
    for (const rule of SENSITIVE_TARGET_RULES) {
      if (rule.re.test(seg)) {
        return { dangerous: true, reason: rule.reason, segment: seg };
      }
    }

    // 剥离 env / VAR=value / sudo 前缀，使规则锚定的段首动词暴露出来
    const bare = seg.replace(/^(?:env\s+|\w+=\S*\s+|sudo\s+)+/, "");
    if (!bare) continue;

    // echo/printf 的参数是数据不是命令，跳过内容规则（敏感目标已在上面查过）
    if (/^(?:echo|printf)\b/.test(bare)) continue;

    for (const rule of CONTENT_RULES) {
      if (rule.re.test(bare)) {
        return { dangerous: true, reason: rule.reason, segment: seg };
      }
    }
  }

  return { dangerous: false };
}
