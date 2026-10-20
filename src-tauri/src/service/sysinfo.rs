//! 主机系统信息采集
//!
//! 通过 `Session::execute` 在已建立的 SSH 会话上执行一段复合 shell，把多块信息一次性
//! 拉回来再在 Rust 侧解析。各段以 `===SECTION:<name>===` 分隔，单段内部按 Linux 习惯
//! 逐行处理。Linux 与 macOS 双路径：Darwin 分支用 sw_vers / sysctl / vm_stat /
//! netstat / top 等命令采集，并伪装成 Linux 的行格式（/proc/stat、meminfo、net/dev），
//! Rust 解析端保持不变。任意一段解析失败时该字段降级为默认值（数字 0 / 字符串空），
//! 不抛错，让前端能继续显示其它字段。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

use crate::errors::AppResult;
use crate::service::ssh as ssh_svc;

#[derive(Debug, Serialize)]
pub struct DiskUsage {
    pub filesystem: String,
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    /// 硬盘型号（sysfs 匹配失败时为 None，序列化时省略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NicStat {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcStat {
    pub pid: u32,
    pub user: String,
    /// procps 路径下的 %CPU；BusyBox /proc 路径下为 0
    pub cpu_percent: f32,
    /// procps 路径下的 %MEM；BusyBox /proc 路径下为 RSS / MemTotal
    pub mem_percent: f32,
    /// 仅在 BusyBox /proc 路径下提供：CPU 累计时间（秒）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_time_secs: Option<f32>,
    /// 仅在 BusyBox /proc 路径下提供：RSS（KB）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mem_rss_kb: Option<u64>,
    pub command: String,
}

/// 温度传感器读数（摄氏度）
#[derive(Debug, Serialize)]
pub struct TempZone {
    /// 展示名：优先传感器 label（如 "Package id 0"），否则 thermal_zone type / hwmon name
    pub label: String,
    pub temp_c: f32,
}

/// 显卡信息
#[derive(Debug, Serialize)]
pub struct GpuInfo {
    /// 卡序号：昇腾为 NPU 编号，NVIDIA/AMD 为枚举序号
    pub index: u32,
    /// "nvidia" / "amd" / "ascend"
    pub vendor: String,
    /// 型号名；拿不到时用 "AMD GPU" 兼底
    pub name: String,
    /// 0 - 100；不支持读取时为 0
    pub util_percent: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub temp_c: f32,
    /// 实时功耗（W）；读不到时省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_watts: Option<f32>,
    /// 功耗上限（W）；无上限概念时省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub power_limit_watts: Option<f32>,
    /// 风扇转速百分比 0-100；不支持读取时省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_percent: Option<f32>,
    /// 风扇转速（RPM）；仅 sysfs 暴露 tach 时有值（AMD）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fan_rpm: Option<u32>,
    /// NVIDIA 性能状态（P0-P12）；其他厂商省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perf_state: Option<String>,
    /// PCIe 链路（如 "Gen4 x16"）；读不到时省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcie: Option<String>,
    /// 健康状态（昇腾 npu-smi 的 OK/Warning/Error）；非昇腾省略
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SysInfo {
    pub hostname: String,
    pub os_pretty: String,
    pub kernel: String,
    pub arch: String,
    pub uptime_secs: u64,
    /// 瞬时占用率：由 collect 用相邻两次 /proc/stat 采样差分回填，拿不到差分时为 0
    pub cpu_percent: f32,
    pub cpu_cores: u32,
    /// CPU 型号（/proc/cpuinfo model name / Processor / Hardware）
    pub cpu_model: String,
    pub mem_total_kb: u64,
    pub mem_used_kb: u64,
    /// 内存类型（如 "DDR4"），dmidecode 不可用时为空
    pub mem_type: String,
    /// 内存频率（如 "3200 MT/s"），dmidecode 不可用时为空
    pub mem_speed: String,
    /// 已插入内存条数，未知时为 0
    pub dimm_count: u32,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
    pub disks: Vec<DiskUsage>,
    /// 累计所有非 lo / docker / br- 网卡的 rx/tx 字节（向后兼容）
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    /// 各网卡明细，用于前端按卡切换
    pub nics: Vec<NicStat>,
    /// 按 CPU 占用排序的前 5 个进程
    pub top_cpu: Vec<ProcStat>,
    /// 按内存占用排序的前 5 个进程
    pub top_mem: Vec<ProcStat>,
    /// 温度传感器读数（thermal_zone + hwmon），无传感器时为空
    pub temps: Vec<TempZone>,
    /// 显卡信息（NVIDIA via nvidia-smi，AMD via amdgpu sysfs），无显卡时为空
    pub gpus: Vec<GpuInfo>,
}

const SCRIPT: &str = r#"
# zsh 默认 nomatch：无匹配的 glob（如 macOS 上不存在的 /sys 路径）是致命错误
# 会终止整个脚本，转成与 sh/bash 一致的空展开；其它 shell 无此命令，静默跳过
[ -n "$ZSH_VERSION" ] && setopt null_glob 2>/dev/null
IS_DARWIN=0
[ "$(uname -s)" = "Darwin" ] && IS_DARWIN=1

echo '===SECTION:hostname==='
hostname 2>/dev/null
echo '===SECTION:os==='
if [ "$IS_DARWIN" = 1 ]; then
  # macOS：sw_vers 拼成 PRETTY_NAME，复用 Linux 解析
  P=$(sw_vers -productName 2>/dev/null)
  [ -n "$P" ] || P=macOS
  V=$(sw_vers -productVersion 2>/dev/null)
  printf 'PRETTY_NAME="%s%s"\n' "$P" "${V:+ $V}"
else
  cat /etc/os-release 2>/dev/null
fi
echo '===SECTION:kernel==='
uname -sr 2>/dev/null
echo '===SECTION:arch==='
uname -m 2>/dev/null
echo '===SECTION:uptime==='
if [ "$IS_DARWIN" = 1 ]; then
  # 无 /proc/uptime：开机时刻（kern.boottime 的 sec 字段）与当前时间求差
  bt=$(sysctl -n kern.boottime 2>/dev/null | sed -n 's/^{ sec = \([0-9]*\).*/\1/p')
  [ -n "$bt" ] && echo $(( $(date +%s) - bt ))
else
  awk '{print $1}' /proc/uptime 2>/dev/null
fi
echo '===SECTION:stat==='
# CPU 占用走相邻两次采样差分：Linux 用 /proc/stat 节拍计数；top -bn1 首帧给的是
# 开机以来的均值，BusyBox top 只有累计帧且 token 是 idle 而非 id，两者都拿不到
# 瞬时占用。Rust 侧按 sid 缓存上次采样做差分，摆脱对 top 是否存在的依赖
if [ "$IS_DARWIN" = 1 ]; then
  # macOS 无 /proc/stat：top -l 1 单帧即瞬时占用，换算成 ×100 的伪节拍行，
  # 交 Rust 侧同一套差分逻辑（两次轮询的百分比之差）
  LC_ALL=C top -l 1 -n 0 2>/dev/null | awk -F'[:,%]' '
    /^CPU usage:/ { printf "cpu %.0f 0 %.0f %.0f\n", $2 * 100, $4 * 100, $6 * 100; exit }'
else
  head -n 1 /proc/stat 2>/dev/null
fi
echo '===SECTION:cores==='
# nproc 反映当前进程的亲和性（taskset/cgroup cpuset 会少报），getconf 取系统在线
# 核数更准；逐级兜底，解析端忽略 0
getconf _NPROCESSORS_ONLN 2>/dev/null
nproc 2>/dev/null
grep -c '^processor' /proc/cpuinfo 2>/dev/null
echo '===SECTION:mem==='
if [ "$IS_DARWIN" = 1 ]; then
  # 无 /proc/meminfo：总量取 hw.memsize；"可用"按活动监视器口径 =
  # 空闲 + speculative + 可回收（purgeable + 文件缓存）页，页大小从表头读
  TOT=$(sysctl -n hw.memsize 2>/dev/null)
  if [ -n "$TOT" ]; then
    vm_stat 2>/dev/null | awk -v tot_kb=$((TOT / 1024)) '
      /page size of/        { ps = $8 + 0 }
      /^Pages free:/        { free = $3 + 0 }
      /^Pages speculative:/ { spec = $3 + 0 }
      /^Pages purgeable:/   { purg = $3 + 0 }
      /^File-backed pages:/ { fb = $3 + 0 }
      END {
        if (ps <= 0) ps = 4096
        avail = int((free + spec + purg + fb) * ps / 1024)
        if (avail > tot_kb) avail = tot_kb
        printf "MemTotal: %d kB\n", tot_kb
        printf "MemAvailable: %d kB\n", avail
      }'
  fi
  # swap：vm.swapusage 形如 "total = 3072.00M  used = 1615.56M  free = ..."
  su=$(sysctl -n vm.swapusage 2>/dev/null)
  ST=$(printf '%s' "$su" | sed -n 's/.*total = \([0-9.]*\)M.*/\1/p')
  SU=$(printf '%s' "$su" | sed -n 's/.*used = \([0-9.]*\)M.*/\1/p')
  if [ -n "$ST" ] && [ -n "$SU" ]; then
    awk -v t="$ST" -v u="$SU" 'BEGIN {
      printf "SwapTotal: %d kB\n", t * 1024
      printf "SwapFree: %d kB\n", (t - u) * 1024
    }'
  fi
else
  cat /proc/meminfo 2>/dev/null
fi
echo '===SECTION:disk==='
LC_ALL=C df -Pk 2>/dev/null | grep -Ev '^(tmpfs|devtmpfs|overlay|aufs|squashfs|udev|run|none)\b' | grep -Ev '^Filesystem\b'
echo '===SECTION:net==='
if [ "$IS_DARWIN" = 1 ]; then
  # 无 /proc/net/dev：netstat -ib 伪装成同形状。首两行空行占位供解析端跳过；
  # 同网卡多地址行字节计数一致，只取首行；Link 行 Address 可能为空（utun 等）
  # 导致列左移，按列数区分 rx/tx 位置
  netstat -ib 2>/dev/null | awk '
    BEGIN { print ""; print "" }
    NR == 1 { next }
    {
      name = $1
      sub(/\*$/, "", name)
      if (name == "Name") next
      if (name ~ /^(lo|awdl|llw|ap|bridge|anpi|stf|gif|pktap|feth|vmnet|vlan)/) next
      if (seen[name]++) next
      if (NF >= 11) { rx = $7 + 0; tx = $10 + 0 } else { rx = $6 + 0; tx = $9 + 0 }
      # 恒 0 的 utun 是系统自带虚拟口，有流量的（活跃 VPN）才保留
      if (name ~ /^utun/ && rx == 0 && tx == 0) next
      printf "%s: %d 0 0 0 0 0 0 0 %d 0 0 0 0 0 0 0\n", name, rx, tx
    }'
else
  cat /proc/net/dev 2>/dev/null
fi
echo '===SECTION:ps==='
LC_ALL=C ps -eo pid,user,pcpu,pmem,comm 2>/dev/null
echo '===SECTION:hertz==='
getconf CLK_TCK 2>/dev/null
echo '===SECTION:proc_uid_name==='
# /etc/passwd 仅取 name:uid，给 /proc 路径用来回填用户名
if [ "$IS_DARWIN" != 1 ]; then
  awk -F: '{print $3 " " $1}' /etc/passwd 2>/dev/null
fi
echo '===SECTION:proc==='
# 遍历 /proc/[0-9]*：每行输出
#   pid <TAB> uid <TAB> rss_kb <TAB> utime_jiffies <TAB> stime_jiffies <TAB> comm
# 兼容 BusyBox awk：用 getline < file 读；comm 取 stat 里第一对 () 之间内容。
if [ "$IS_DARWIN" != 1 ]; then
LC_ALL=C awk '
BEGIN {
  cmd = "ls -1 /proc 2>/dev/null"
  while ((cmd | getline pid) > 0) {
    if (pid !~ /^[0-9]+$/) continue

    statf = "/proc/" pid "/stat"
    if ((getline line < statf) <= 0) { close(statf); continue }
    close(statf)

    # comm 在第一对 () 内（可能含空格、特殊字符）；rstrchr 找最后一个 )
    lp = index(line, "(")
    rp = 0
    for (i = length(line); i >= 1; i--) {
      if (substr(line, i, 1) == ")") { rp = i; break }
    }
    if (lp == 0 || rp == 0 || rp <= lp) continue
    comm = substr(line, lp+1, rp-lp-1)

    # ) 之后是 state utime[12] stime[13]... 的剩余字段（按 1-based 数）
    rest = substr(line, rp+2)
    nf = split(rest, f, " ")
    # man proc：pid(1) comm(2) state(3) ppid(4) ... utime(14) stime(15) ...
    # ) 之后从 state 开始，所以 state=f[1], utime=f[12], stime=f[13]
    if (nf < 13) continue
    utime = f[12] + 0
    stime = f[13] + 0

    # /proc/<pid>/status 取 Uid 与 VmRSS
    sf = "/proc/" pid "/status"
    uid = "0"; rss = 0
    while ((getline s < sf) > 0) {
      if (s ~ /^Uid:/) { split(s, u, /[ \t]+/); uid = u[2] }
      else if (s ~ /^VmRSS:/) { split(s, r, /[ \t]+/); rss = r[2] + 0 }
    }
    close(sf)

    print pid "\t" uid "\t" rss "\t" utime "\t" stime "\t" comm
  }
  close(cmd)
}
' 2>/dev/null
fi
echo '===SECTION:temp==='
# thermal_zone：CPU 封装 / ACPI 温度，无需任何工具
for z in /sys/class/thermal/thermal_zone*; do
  [ -f "$z/temp" ] || continue
  ty=$(cat "$z/type" 2>/dev/null)
  te=$(cat "$z/temp" 2>/dev/null)
  [ -n "$te" ] && printf 'TZ\t%s\t%s\n' "$ty" "$te"
done
# hwmon：补充 NVMe / 核心温度等带 label 的传感器
for h in /sys/class/hwmon/hwmon*; do
  [ -d "$h" ] || continue
  nm=$(cat "$h/name" 2>/dev/null)
  for tf in "$h"/temp*_input; do
    [ -f "$tf" ] || continue
    te=$(cat "$tf" 2>/dev/null)
    lb=$(cat "${tf%_input}_label" 2>/dev/null)
    printf 'HW\t%s\t%s\t%s\n' "$nm" "$lb" "$te"
  done
done
# SATA/SCSI 硬盘温度：依赖 smartctl（通常需 root，无权限时自动跳过）
# 非交互 SSH 的 PATH 可能不含 /usr/sbin / /usr/local/sbin，补绝对路径兜底
SMARTCTL=$(command -v smartctl 2>/dev/null || ls /usr/sbin/smartctl /usr/local/sbin/smartctl 2>/dev/null | head -n 1)
if [ -n "$SMARTCTL" ] && [ -x "$SMARTCTL" ]; then
  for b in /sys/block/sd*; do
    dev="/dev/$(basename "$b")"
    [ -e "$dev" ] || continue
    te=$("$SMARTCTL" -A -n standby "$dev" 2>/dev/null | awk '
      /Current Drive Temperature/ {print $4; exit}
      /Temperature_Celsius/ {print $10; exit}
      /Temperature:/ {print $2; exit}
    ')
    [ -n "$te" ] && printf 'SD\t%s\t%s\n' "$(basename "$b")" "$te"
  done
fi
echo '===SECTION:gpu==='
# NVIDIA：优先带风扇/功耗/性能状态的扩展查询；极老驱动不认识新字段会整条失败，
# 失败时退回原始五字段。PCIe 链路单独查询（同样可能不被老驱动支持），
# 输出 NVPCIE 行由 Rust 侧按行序与 NV 卡配对，行数不齐时整组丢弃。
if command -v nvidia-smi >/dev/null 2>&1; then
  NVOUT=$(nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,fan.speed,power.draw,power.limit,pstate --format=csv,noheader,nounits 2>/dev/null)
  if [ -z "$NVOUT" ]; then
    NVOUT=$(nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader,nounits 2>/dev/null)
  fi
  printf '%s\n' "$NVOUT" | while IFS=, read -r n u mu mt tp fn pw pl ps; do
    [ -n "$n" ] || continue
    n=$(printf '%s' "$n" | sed 's/^ *//; s/ *$//')
    printf 'NV\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$n" "$u" "$mu" "$mt" "$tp" "$fn" "$pw" "$pl" "$ps"
  done
  nvidia-smi --query-gpu=pcie.link.gen.current,pcie.link.width.current --format=csv,noheader,nounits 2>/dev/null |
  while IFS=, read -r gen w; do
    [ -n "$gen" ] || continue
    printf 'NVPCIE\t%s\t%s\n' "$gen" "$w"
  done
fi
# AMD（amdgpu 驱动）：走 drm sysfs，不依赖 rocm-smi
for d in /sys/class/drm/card[0-9]*; do
  [ -d "$d/device" ] || continue
  ven=$(cat "$d/device/vendor" 2>/dev/null)
  [ "$ven" = "0x1002" ] || continue
  busy=$(cat "$d/device/gpu_busy_percent" 2>/dev/null)
  te=$(cat "$d/device"/hwmon/hwmon*/temp1_input 2>/dev/null | head -n 1)
  vu=$(cat "$d/device/mem_info_vram_used" 2>/dev/null)
  vt=$(cat "$d/device/mem_info_vram_total" 2>/dev/null)
  hwd=$(ls -d "$d/device"/hwmon/hwmon* 2>/dev/null | head -n 1)
  fp=""; fr=""; pw=""; pl=""
  if [ -n "$hwd" ]; then
    # 功耗：新内核 power1_average 与旧 power1_input 二选一，微瓦转瓦
    pwr=$(cat "$hwd/power1_average" 2>/dev/null)
    [ -n "$pwr" ] || pwr=$(cat "$hwd/power1_input" 2>/dev/null)
    [ -n "$pwr" ] && pw=$(awk -v r="$pwr" 'BEGIN{printf "%.1f", r/1000000}')
    # 功耗上限：power1_cap 缺失时用板上限 power1_cap_max
    cap=$(cat "$hwd/power1_cap" 2>/dev/null)
    [ -n "$cap" ] || cap=$(cat "$hwd/power1_cap_max" 2>/dev/null)
    [ -n "$cap" ] && pl=$(awk -v r="$cap" 'BEGIN{printf "%.0f", r/1000000}')
    # 风扇：pwm1（0-255 占空比）换算百分比；fan1_input 直接是 RPM
    pwm=$(cat "$hwd/pwm1" 2>/dev/null)
    [ -n "$pwm" ] && fp=$(awk -v p="$pwm" 'BEGIN{printf "%.0f", p/255*100}')
    fr=$(cat "$hwd/fan1_input" 2>/dev/null)
  fi
  addr=$(basename "$(readlink -f "$d/device")" 2>/dev/null)
  name=$(lspci -s "$addr" 2>/dev/null | sed 's/^.*: //')
  [ -n "$name" ] || name="AMD GPU"
  printf 'AM\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$busy" "$vu" "$vt" "$te" "$fp" "$fr" "$pw" "$pl"
done
# 华为昇腾 NPU：npu-smi 表格原样输出，由 Rust 侧解析
# npu-smi 常装在 /usr/local/sbin，非交互 SSH PATH 可能不含该目录，补绝对路径兜底
NPU_SM=$(command -v npu-smi 2>/dev/null || ls /usr/local/sbin/npu-smi /usr/sbin/npu-smi 2>/dev/null | head -n 1)
if [ -n "$NPU_SM" ] && [ -x "$NPU_SM" ]; then
  "$NPU_SM" info 2>/dev/null
fi
echo '===SECTION:cpuinfo==='
# CPU 型号：x86 优先 model name；ARM /proc/cpuinfo 各家不统一，逐级回退到 lscpu
if [ "$IS_DARWIN" = 1 ]; then
  sysctl -n machdep.cpu.brand_string 2>/dev/null
else
grep -m1 '^model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2-
grep -m1 '^Model Name' /proc/cpuinfo 2>/dev/null | cut -d: -f2-
if command -v lscpu >/dev/null 2>&1; then
  LC_ALL=C lscpu 2>/dev/null | grep -m1 '^Model name' | cut -d: -f2-
fi
grep -m1 '^Processor' /proc/cpuinfo 2>/dev/null | cut -d: -f2-
grep -m1 '^Hardware' /proc/cpuinfo 2>/dev/null | cut -d: -f2-
grep -m1 '^Model' /proc/cpuinfo 2>/dev/null | cut -d: -f2-
fi
echo '===SECTION:dmi_mem==='
# 内存规格：依赖 dmidecode（需 root），失败时留空由前端隐藏
if command -v dmidecode >/dev/null 2>&1; then
  dmidecode -t memory 2>/dev/null | awk -F': *' '
    /^[[:space:]]*Size:/ && $2 != "" && $2 != "No Module Installed" { n++ }
    /^[[:space:]]*Type:/ && $2 != "" && $2 != "Unknown" { t = $2 }
    /^[[:space:]]*Configured Memory Speed:/ && $2 != "" && $2 != "Unknown" { cs = $2 }
    /^[[:space:]]*Speed:/ && $2 != "" && $2 != "Unknown" { s = $2 }
    END { printf "DIM\t%d\t%s\t%s\n", n+0, t, (cs != "" ? cs : s) }
  '
fi
echo '===SECTION:blkmod==='
# 硬盘型号：sysfs 免工具获取（SATA/SCSI、NVMe、virtio、mmc/eMMC）
for b in /sys/block/sd* /sys/block/nvme* /sys/block/vd* /sys/block/mmcblk*; do
  [ -e "$b" ] || continue
  dev=$(basename "$b")
  case "$dev" in loop*|ram*|zram*|md*|dm-*) continue ;; esac
  m=$(cat "$b/device/model" 2>/dev/null)
  [ -n "$m" ] || continue
  printf 'BLK\t%s\t%s\n' "$dev" "$m"
done
echo '===SECTION:end==='
"#;

/// /proc/stat 聚合 cpu 行的节拍采样（相邻两次差分出瞬时占用率）
#[derive(Debug, Clone, Copy, PartialEq)]
struct CpuTicks {
    total: u64,
    idle: u64,
}

pub async fn collect(sid: &str) -> AppResult<SysInfo> {
    let session = ssh_svc::get_client(sid).await?;
    let result = session.execute(SCRIPT).await?;
    let (mut info, cpu_ticks) = parse(&result.stdout);
    info.cpu_percent = cpu_percent_for(sid, cpu_ticks);
    Ok(info)
}

fn cpu_cache() -> &'static Mutex<HashMap<String, (CpuTicks, Instant)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (CpuTicks, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 用相邻两次 /proc/stat 采样差分计算瞬时 CPU 占用率。
/// 首次采样、与上次间隔过短/过长（常规轮询 1.5s、失败退避 5s）、或计数回绕
/// （主机重启）时差分不可信，本次返回 0，由下一次轮询补上。
fn cpu_percent_for(sid: &str, ticks: Option<CpuTicks>) -> f32 {
    let Some(ticks) = ticks else {
        return 0.0;
    };
    let mut cache = cpu_cache().lock().unwrap();
    let now = Instant::now();
    let prev = cache.get(sid).copied();
    // sid 断开后没有删除钩子：条目膨胀时顺手清理 1 分钟未活动的旧会话
    if cache.len() > 256 {
        cache.retain(|_, s| now.duration_since(s.1).as_secs() < 60);
    }
    cache.insert(sid.to_string(), (ticks, now));
    prev.and_then(|(pt, pat)| cpu_pct_between(pt, ticks, now.duration_since(pat).as_secs_f64()))
        .unwrap_or(0.0)
}

/// 两次采样的差分：busy = 1 - idle/total
fn cpu_pct_between(prev: CpuTicks, cur: CpuTicks, dt_secs: f64) -> Option<f32> {
    if !(0.2..=15.0).contains(&dt_secs) || cur.total <= prev.total {
        return None;
    }
    let d_total = cur.total - prev.total;
    let d_idle = cur.idle.saturating_sub(prev.idle).min(d_total);
    Some(clamp_pct((d_total - d_idle) as f32 / d_total as f32 * 100.0))
}

/// 解析 /proc/stat 首行：`cpu  user nice system idle iowait irq softirq steal ...`
/// guest/guest_nice 已计入 user/nice（man proc），total 只累加前 8 个字段；
/// idle 连同 iowait 一起视为非忙碌（与常见监控口径一致，iowait 不虚增 CPU%）
fn parse_cpu_stat(lines: &[&str]) -> Option<CpuTicks> {
    for l in lines {
        let mut it = l.split_whitespace();
        if it.next() != Some("cpu") {
            continue; // 跳过 cpu0/cpu1 等单核行
        }
        let f: Vec<u64> = it.map(|v| v.parse().unwrap_or(0)).collect();
        if f.len() < 4 {
            continue; // 连 idle 字段都没有，无法差分
        }
        let idle = f[3] + f.get(4).copied().unwrap_or(0);
        let total = f.iter().take(8).sum();
        return Some(CpuTicks { total, idle });
    }
    None
}

/// 解析脚本输出。cpu_percent 置 0 占位，由 collect 用相邻两次 /proc/stat
/// 采样差分回填；同时把本次采样带出去。
fn parse(text: &str) -> (SysInfo, Option<CpuTicks>) {
    let mut sections: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    let mut current: Option<&str> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("===SECTION:") {
            if let Some(name) = rest.strip_suffix("===") {
                current = Some(name);
                sections.entry(name).or_default();
            }
            continue;
        }
        if let Some(name) = current {
            sections.get_mut(name).unwrap().push(line);
        }
    }

    let mut take = |k: &str| -> Vec<&str> { sections.remove(k).unwrap_or_default() };

    let hostname = take("hostname")
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let os_pretty = parse_os_pretty(&take("os"));
    let kernel = take("kernel")
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let arch = take("arch")
        .iter()
        .find(|l| !l.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let uptime_secs = take("uptime")
        .iter()
        .find_map(|l| l.split_whitespace().next())
        .and_then(|s| s.parse::<f64>().ok())
        .map(|f| f as u64)
        .unwrap_or(0);

    let cpu_ticks = parse_cpu_stat(&take("stat"));
    let cpu_cores = take("cores")
        .iter()
        .find_map(|l| l.trim().parse::<u32>().ok().filter(|v| *v > 0))
        .unwrap_or(0);
    let cpu_model = parse_cpu_model(&take("cpuinfo"));
    let (mem_type, mem_speed, dimm_count) = parse_dmi_mem(&take("dmi_mem"));

    let mut disks = parse_disks(&take("disk"));
    attach_disk_models(&mut disks, parse_blk_models(&take("blkmod")));
    let (mem_total_kb, mem_used_kb, swap_total_kb, swap_used_kb) = parse_mem(&take("mem"));
    let nics = parse_nics(&take("net"));
    let net_rx_bytes = nics.iter().map(|n| n.rx_bytes).sum();
    let net_tx_bytes = nics.iter().map(|n| n.tx_bytes).sum();
    let (top_cpu, top_mem) = parse_top(
        &take("ps"),
        &take("hertz"),
        &take("proc_uid_name"),
        &take("proc"),
        mem_total_kb,
    );
    let temps = parse_temps(&take("temp"));
    let gpus = parse_gpus(&take("gpu"));

    (
        SysInfo {
            hostname,
            os_pretty,
            kernel,
            arch,
            uptime_secs,
            cpu_percent: 0.0,
            cpu_cores,
            cpu_model,
            mem_total_kb,
            mem_used_kb,
            mem_type,
            mem_speed,
            dimm_count,
            swap_total_kb,
            swap_used_kb,
            disks,
            net_rx_bytes,
            net_tx_bytes,
            nics,
            top_cpu,
            top_mem,
            temps,
            gpus,
        },
        cpu_ticks,
    )
}

fn parse_os_pretty(lines: &[&str]) -> String {
    for l in lines {
        if let Some(rest) = l.strip_prefix("PRETTY_NAME=") {
            return rest.trim().trim_matches('"').to_string();
        }
    }
    // 退化：拼 NAME + VERSION
    let mut name = String::new();
    let mut version = String::new();
    for l in lines {
        if let Some(v) = l.strip_prefix("NAME=") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = l.strip_prefix("VERSION=") {
            version = v.trim().trim_matches('"').to_string();
        }
    }
    if !name.is_empty() {
        if version.is_empty() {
            name
        } else {
            format!("{name} {version}")
        }
    } else {
        String::new()
    }
}

fn parse_cpu_model(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

/// 解析 dmidecode 汇总行：`DIM\t<已插条数>\t<类型>\t<频率>`
fn parse_dmi_mem(lines: &[&str]) -> (String, String, u32) {
    for l in lines {
        let cols: Vec<&str> = l.split('\t').collect();
        if cols.len() < 4 || cols[0] != "DIM" {
            continue;
        }
        let count = cols[1].trim().parse::<u32>().unwrap_or(0);
        let mtype = cols[2].trim().to_string();
        let speed = cols[3].trim().to_string();
        if count == 0 && mtype.is_empty() {
            continue;
        }
        return (mtype, speed, count);
    }
    (String::new(), String::new(), 0)
}

/// 解析 sysfs 硬盘型号行：`BLK\t<设备名>\t<型号>`
fn parse_blk_models(lines: &[&str]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for l in lines {
        let cols: Vec<&str> = l.splitn(3, '\t').collect();
        if cols.len() < 3 || cols[0] != "BLK" {
            continue;
        }
        let model = cols[2].trim().to_string();
        if model.is_empty() {
            continue;
        }
        out.push((cols[1].to_string(), model));
    }
    out
}

/// 把 sysfs 设备型号按设备名前缀匹配回填到 df 盘点上。
/// df 的 filesystem 形如 /dev/sda1、/dev/nvme0n1p2，取最长匹配键。
fn attach_disk_models(disks: &mut [DiskUsage], models: Vec<(String, String)>) {
    if models.is_empty() {
        return;
    }
    for d in disks.iter_mut() {
        let Some(devname) = d.filesystem.strip_prefix("/dev/") else {
            continue;
        };
        let mut best: Option<&(String, String)> = None;
        for kv in &models {
            // sda 匹配 sda1；nvme0n1 匹配 nvme0n1p2（分区标记 p）
            let matched = devname == kv.0
                || devname.starts_with(&format!("{}p", kv.0))
                || (devname.starts_with(&kv.0)
                    && !kv.0.starts_with("nvme")
                    && !kv.0
                        .chars()
                        .last()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false));
            let better = match best {
                None => true,
                Some(b) => kv.0.len() > b.0.len(),
            };
            if matched && better {
                best = Some(kv);
            }
        }
        if let Some((_, m)) = best {
            d.model = Some(m.clone());
        }
    }
}

/// 解析温度段：`TZ\t<type>\t<毫摄氏度>` / `HW\t<hwmon名>\t<label>\t<毫摄氏度>`
/// / `SD\t<盘符>\t<摄氏度（smartctl 原始值）>`。
/// hwmon 与 thermal_zone 可能重复暴露同一传感器，按 (label, 温度) 去重。
fn parse_temps(lines: &[&str]) -> Vec<TempZone> {
    let mut out: Vec<TempZone> = Vec::new();
    let mut seen: std::collections::HashSet<(String, u32)> = std::collections::HashSet::new();
    for l in lines {
        let cols: Vec<&str> = l.split('\t').collect();
        let (label_raw, raw): (&str, &str) = match cols.as_slice() {
            ["TZ", ty, te] => (ty, te),
            ["HW", name, lb, te] => {
                let label = if lb.trim().is_empty() { name } else { lb };
                (label, te)
            }
            // smartctl：SATA/SCSI 盘，值已是摄氏度
            ["SD", dev, te] => (dev, te),
            _ => continue,
        };
        let label = label_raw.trim();
        if label.is_empty() {
            continue;
        }
        let Some(milli) = raw.trim().parse::<f32>().ok() else {
            continue;
        };
        // TZ/HW 为毫摄氏度，SD 已是摄氏度；SD 值一般 < 100，TZ 除以 1000 后也在同区间
        let temp_c = if cols[0] == "SD" {
            milli
        } else {
            milli / 1000.0
        };
        // 过滤无效读数：挂载但无数据的传感器常报 0
        if !temp_c.is_finite() || temp_c <= 0.0 || temp_c > 150.0 {
            continue;
        }
        // 以 0.1°C 粒度去重
        let key = (label.to_string(), (temp_c * 10.0).round() as u32);
        if !seen.insert(key) {
            continue;
        }
        out.push(TempZone {
            label: label.to_string(),
            temp_c: (temp_c * 10.0).round() / 10.0,
        });
    }
    // 按温度从高到低，最热的排前面
    out.sort_by(|a, b| {
        b.temp_c
            .partial_cmp(&a.temp_c)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// 解析 GPU 段：每行 `NV\t<name>\t<util%>\t<used MiB>\t<total MiB>\t<°C>\t[<fan%>\t<W>\t<上限W>\t<P态>]`
/// 或 `AM\t<name>\t<busy%>\t<used bytes>\t<total bytes>\t<毫摄氏度>\t[<fan%>\t<RPM>\t<W>\t<上限W>]`，
/// 另有 `NVPCIE\t<gen>\t<width>` 行与 NV 卡按行序配对。
/// 方括号内为扩展字段（风扇/功耗/P态），老脚本或兜底查询没有时缺省。
/// 字段可能为 "[N/A]" / 空，宽松解析，取不到置 0 / None。
fn parse_gpus(lines: &[&str]) -> Vec<GpuInfo> {
    let mut out = Vec::new();
    // PCIe 行先收集，与 NV 行数一致才按序配对（避免 nvidia-smi 两次查询间卡数变化错配）
    let nv_pcie: Vec<Option<String>> = lines
        .iter()
        .filter(|l| l.starts_with("NVPCIE\t"))
        .map(|l| parse_pcie_line(l))
        .collect();
    let nv_total = lines
        .iter()
        .filter(|l| l.split('\t').next() == Some("NV"))
        .count();
    let pcie_paired = nv_total > 0 && nv_total == nv_pcie.len();
    let mut nv_seen = 0usize;
    // 昇腾 NPU 行与 Chip 行分两行输出，先用 pending 缓存配对：
    // (型号, 温度, NPU 编号, 功率W, 健康态)
    let mut ascend_pending: Vec<(String, f32, u32, Option<f32>, String)> = Vec::new();
    for l in lines {
        // npu-smi 表格行（含 '|'，非制表符格式）
        if l.contains('|') {
            if let Some(gpu) = ascend_table_line(l, &mut ascend_pending) {
                out.push(gpu);
            }
            continue;
        }
        if l.starts_with("NVPCIE\t") {
            continue;
        }
        let cols: Vec<&str> = l.split('\t').collect();
        if cols.len() < 6 {
            continue;
        }
        let name = cols[1].trim().to_string();
        if name.is_empty() {
            continue;
        }
        let f32_of = |s: &str| -> f32 {
            s.trim()
                .trim_start_matches('[')
                .parse::<f32>()
                .unwrap_or(0.0)
        };
        let u64_of = |s: &str| -> u64 { s.trim().parse::<u64>().unwrap_or(0) };
        // 扩展字段：解析失败（[N/A] / 空）返回 None，而不是降级成 0
        let opt_f32 = |i: usize| -> Option<f32> {
            cols.get(i).and_then(|s| s.trim().parse::<f32>().ok())
        };
        let opt_u32 = |i: usize| -> Option<u32> {
            cols.get(i).and_then(|s| s.trim().parse::<u32>().ok())
        };
        let gpu = match cols[0] {
            "NV" => {
                let temp = f32_of(cols[5]);
                let perf_state = cols
                    .get(9)
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && !s.contains("N/A"))
                    .map(|s| s.to_string());
                let pcie = if pcie_paired {
                    nv_pcie.get(nv_seen).cloned().flatten()
                } else {
                    None
                };
                nv_seen += 1;
                GpuInfo {
                    index: out.len() as u32,
                    vendor: "nvidia".into(),
                    name,
                    util_percent: clamp_pct(f32_of(cols[2])),
                    mem_used_mb: u64_of(cols[3]),
                    mem_total_mb: u64_of(cols[4]),
                    temp_c: if (0.0..=150.0).contains(&temp) {
                        temp
                    } else {
                        0.0
                    },
                    power_watts: opt_f32(7),
                    power_limit_watts: opt_f32(8),
                    fan_percent: opt_f32(6),
                    fan_rpm: None,
                    perf_state,
                    pcie,
                    health: None,
                }
            }
            "AM" => {
                let temp_milli = f32_of(cols[5]);
                let temp = if temp_milli > 200.0 {
                    temp_milli / 1000.0 // hwmon 毫摄氏度
                } else {
                    temp_milli // 部分驱动直接给摄氏度
                };
                GpuInfo {
                    index: out.len() as u32,
                    vendor: "amd".into(),
                    name,
                    util_percent: clamp_pct(f32_of(cols[2])),
                    mem_used_mb: u64_of(cols[3]) / (1024 * 1024),
                    mem_total_mb: u64_of(cols[4]) / (1024 * 1024),
                    temp_c: if (0.0..=150.0).contains(&temp) {
                        temp
                    } else {
                        0.0
                    },
                    power_watts: opt_f32(8),
                    power_limit_watts: opt_f32(9),
                    fan_percent: opt_f32(6),
                    fan_rpm: opt_u32(7),
                    perf_state: None,
                    pcie: None,
                    health: None,
                }
            }
            _ => continue,
        };
        out.push(gpu);
    }
    out
}

/// 解析 `NVPCIE\t<gen>\t<width>`：兼容 nounits 的 "4" / "16" 与带单位的 "Gen4" / "x16"，
/// 两段都提取到数字才成 "Gen4 x16"，否则放弃。
fn parse_pcie_line(l: &str) -> Option<String> {
    let cols: Vec<&str> = l.split('\t').collect();
    if cols.len() < 3 {
        return None;
    }
    let num_of = |s: &str| -> Option<u32> {
        let digits: String = s.trim().chars().skip_while(|c| !c.is_ascii_digit()).collect();
        digits.parse::<u32>().ok()
    };
    let gen = num_of(cols[1])?;
    let width = num_of(cols[2])?;
    if gen == 0 || width == 0 {
        return None;
    }
    Some(format!("Gen{gen} x{width}"))
}

/// 判断是否为总线号 token：老格式 `0x0000`，新格式 `0000:C1:00.0` / `C1:00.0`
fn is_pci_bus(tk: &str) -> bool {
    if tk.starts_with("0x") {
        return true;
    }
    let parts: Vec<&str> = tk.split(':').collect();
    parts.len() >= 2
        && parts[..parts.len() - 1]
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 4 && p.chars().all(|c| c.is_ascii_hexdigit()))
        && parts[parts.len() - 1]
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '.')
        && parts[parts.len() - 1].contains('.')
}

/// 从 token 流里提取所有 `used / total` 对，兼容 `0 / 0`、`60465/ 65536`、`736  / 32268` 等排版
fn mem_pairs(tokens: &[&str]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let tk = tokens[i];
        if tk == "/" {
            if i >= 1 {
                if let (Ok(a), Some(b)) = (
                    tokens[i - 1].parse::<u64>(),
                    tokens.get(i + 1).and_then(|s| s.parse::<u64>().ok()),
                ) {
                    out.push((a, b));
                    i += 2;
                    continue;
                }
            }
        } else if let Some(u) = tk.strip_suffix('/') {
            if let (Ok(a), Some(b)) = (
                u.parse::<u64>(),
                tokens.get(i + 1).and_then(|s| s.parse::<u64>().ok()),
            ) {
                out.push((a, b));
                i += 2;
                continue;
            }
        } else if let Some(b) = tk.strip_prefix('/') {
            if i >= 1 {
                if let (Ok(a), Ok(t)) = (tokens[i - 1].parse::<u64>(), b.parse::<u64>()) {
                    out.push((a, t));
                    i += 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// 解析 npu-smi info 表格行。
/// NPU 行：`| 0  910B4 | OK | 68.5  45 ... |` → 记下（型号, 温度, 功率, 健康态）待配对；
/// Chip 行（含总线号）：`| 0 | 0x0000 | 0 | 651 / 1517 ... |` → 取 AICore%
/// 与显存（取最后一个 used / total 对，多芯片卡上即 HBM），与前一条 NPU 行合成一条记录。
/// 老版本（如 20.x）一行格里塞多个字段，拆 '|' 后再按空白二次切分统一处理。
fn ascend_table_line(
    l: &str,
    pending: &mut Vec<(String, f32, u32, Option<f32>, String)>,
) -> Option<GpuInfo> {
    let t = l.trim();
    if t.starts_with('+') {
        return None;
    }
    let tokens: Vec<&str> = t
        .split(|c: char| c == '|' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    // 首个 token 必须是数字索引，否则是表头 / 版本行
    if tokens.len() < 4 || tokens[0].parse::<u32>().is_err() {
        return None;
    }
    let bus_idx = tokens.iter().position(|tk| is_pci_bus(tk));
    match bus_idx {
        Some(ix) => {
            let util = tokens
                .get(ix + 1)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            // 取最后一个 used / total 对：多芯片卡上即 HBM 显存
            let (mem_used, mem_total) = mem_pairs(&tokens)
                .last()
                .copied()
                .unwrap_or((0, 0));
            let (name, temp_c, npu_index, power_watts, health) = pending
                .pop()
                .unwrap_or_else(|| {
                    (
                        "Ascend NPU".to_string(),
                        0.0,
                        tokens[0].parse::<u32>().unwrap_or(0),
                        None,
                        String::new(),
                    )
                });
            Some(GpuInfo {
                index: npu_index,
                vendor: "ascend".into(),
                name,
                util_percent: clamp_pct(util),
                mem_used_mb: mem_used,
                mem_total_mb: mem_total,
                temp_c: temp_c.clamp(0.0, 150.0),
                power_watts,
                power_limit_watts: None,
                fan_percent: None,
                fan_rpm: None,
                perf_state: None,
                pcie: None,
                health: if health.is_empty() { None } else { Some(health) },
            })
        }
        None => {
            // NPU 行：tokens = [索引, 型号, 健康态, 功率, 温度, ...]
            // 健康态（OK/Warning/Error）非数字；底部进程表行第 3 个 token 是数字 PID，跳过避免污染配对
            if tokens[2].parse::<u32>().is_ok() {
                return None;
            }
            let name = tokens[1].to_string();
            let temp_c = tokens
                .get(4)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            let power_watts = tokens.get(3).and_then(|s| s.parse::<f32>().ok());
            let npu_index = tokens[0].parse::<u32>().unwrap_or(0);
            pending.push((name, temp_c, npu_index, power_watts, tokens[2].to_string()));
            None
        }
    }
}

fn clamp_pct(v: f32) -> f32 {
    if !v.is_finite() {
        0.0
    } else if v < 0.0 {
        0.0
    } else if v > 100.0 {
        100.0
    } else {
        v
    }
}

fn parse_mem(lines: &[&str]) -> (u64, u64, u64, u64) {
    let mut mem_total = 0u64;
    let mut mem_avail = 0u64;
    let mut mem_free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut sreclaim = 0u64;
    let mut swap_total = 0u64;
    let mut swap_free = 0u64;

    for l in lines {
        let mut it = l.split_whitespace();
        let key = it.next().unwrap_or("");
        let val = it.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => mem_total = val,
            "MemAvailable:" => mem_avail = val,
            "MemFree:" => mem_free = val,
            "Buffers:" => buffers = val,
            "Cached:" => cached = val,
            "SReclaimable:" => sreclaim = val,
            "SwapTotal:" => swap_total = val,
            "SwapFree:" => swap_free = val,
            _ => {}
        }
    }

    let used = if mem_avail > 0 {
        mem_total.saturating_sub(mem_avail)
    } else {
        // 老内核兜底：total - free - buffers - cached - sreclaimable
        mem_total
            .saturating_sub(mem_free)
            .saturating_sub(buffers)
            .saturating_sub(cached)
            .saturating_sub(sreclaim)
    };
    let swap_used = swap_total.saturating_sub(swap_free);

    (mem_total, used, swap_total, swap_used)
}

/// 挂载点前缀黑名单：第一列 fs 名过滤不全（如 efivarfs、bpffs），这些目录下
/// 不会挂真实数据盘。注意不能加 /run——桌面发行版的 udisks 会把 U 盘挂到
/// /run/media/<user>/<label>。
const PSEUDO_MOUNT_PREFIXES: [&str; 3] = ["/proc", "/sys", "/dev"];

fn is_pseudo_mount(mount: &str) -> bool {
    PSEUDO_MOUNT_PREFIXES
        .iter()
        .any(|p| mount == *p || mount.starts_with(&format!("{p}/")))
}

fn parse_disks(lines: &[&str]) -> Vec<DiskUsage> {
    let mut out = Vec::new();
    for l in lines {
        // df -Pk 输出列：Filesystem 1024-blocks Used Available Capacity Mounted-on
        // 表头与虚拟 fs 已在 shell 端 grep 过滤；这里直接逐行解析
        let cols: Vec<&str> = l.split_whitespace().collect();
        if cols.len() < 6 {
            continue;
        }
        let fs = cols[0].to_string();
        let total_kb = cols[1].parse::<u64>().unwrap_or(0);
        let used_kb = cols[2].parse::<u64>().unwrap_or(0);
        let mount = cols[5..].join(" ");
        if total_kb == 0 {
            continue;
        }
        out.push(DiskUsage {
            filesystem: fs,
            mount,
            total_bytes: total_kb.saturating_mul(1024),
            used_bytes: used_kb.saturating_mul(1024),
            model: None,
        });
    }
    out.retain(|d| !is_pseudo_mount(&d.mount));
    // 把 / 排第一
    out.sort_by_key(|d| if d.mount == "/" { 0 } else { 1 });
    out
}

fn parse_nics(lines: &[&str]) -> Vec<NicStat> {
    let mut out = Vec::new();
    // /proc/net/dev 前两行是表头
    for l in lines.iter().skip(2) {
        let trimmed = l.trim_start();
        let Some((iface, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let iface = iface.trim();
        if iface.is_empty()
            || iface == "lo"
            || iface.starts_with("docker")
            || iface.starts_with("br-")
            || iface.starts_with("veth")
        {
            continue;
        }
        let cols: Vec<&str> = rest.split_whitespace().collect();
        if cols.len() < 16 {
            continue;
        }
        let rx = cols[0].parse::<u64>().unwrap_or(0);
        let tx = cols[8].parse::<u64>().unwrap_or(0);
        out.push(NicStat {
            name: iface.to_string(),
            rx_bytes: rx,
            tx_bytes: tx,
        });
    }
    // 名字稳定排序，方便前端切换时位置不跳
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 解析 ps / /proc 输出为 (top_cpu_5, top_mem_5)。
///
/// 优先尝试 procps 风格的 `ps -eo pid,user,pcpu,pmem,comm`（CentOS / Debian / Ubuntu）；
/// 若该段为空（BusyBox / Alpine 不认 `-eo`），fallback 到 `/proc` 段：
/// 每行 `pid \t uid \t rss_kb \t utime \t stime \t comm`。
///
/// 走 /proc 路径时：
/// - cpu_percent 字段用 (utime+stime)/Hertz 的"CPU 累计时间秒"代替（前端显示 mm:ss）
/// - mem_percent 字段用 rss_kb / mem_total_kb * 100 计算
/// - user 通过 /etc/passwd 提供的 uid→name 映射回填，缺失时直接显示数字 uid
fn parse_top(
    ps_lines: &[&str],
    hertz_lines: &[&str],
    passwd_lines: &[&str],
    proc_lines: &[&str],
    mem_total_kb: u64,
) -> (Vec<ProcStat>, Vec<ProcStat>) {
    // 先试 procps 路径
    if let Some(pair) = parse_top_procps(ps_lines) {
        return pair;
    }
    // BusyBox 路径
    parse_top_proc(proc_lines, hertz_lines, passwd_lines, mem_total_kb)
}

/// 解析 `ps -eo pid,user,pcpu,pmem,comm`。任何一行能成功解析就认为该段可用。
fn parse_top_procps(lines: &[&str]) -> Option<(Vec<ProcStat>, Vec<ProcStat>)> {
    let mut procs: Vec<ProcStat> = Vec::new();
    for l in lines {
        let line = l.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 {
            continue;
        }
        // 跳过表头
        let Ok(pid) = cols[0].parse::<u32>() else {
            continue;
        };
        let user = cols[1].to_string();
        let cpu = cols[2].parse::<f32>().unwrap_or(0.0);
        let mem = cols[3].parse::<f32>().unwrap_or(0.0);
        let command = cols[4..].join(" ");
        procs.push(ProcStat {
            pid,
            user,
            cpu_percent: cpu,
            mem_percent: mem,
            cpu_time_secs: None,
            mem_rss_kb: None,
            command,
        });
    }
    if procs.is_empty() {
        return None;
    }
    Some(rank_procs(procs))
}

/// 解析 awk 输出的 /proc 段。
fn parse_top_proc(
    proc_lines: &[&str],
    hertz_lines: &[&str],
    passwd_lines: &[&str],
    mem_total_kb: u64,
) -> (Vec<ProcStat>, Vec<ProcStat>) {
    let hertz: f32 = hertz_lines
        .iter()
        .find_map(|l| l.trim().parse::<u32>().ok())
        .unwrap_or(100) as f32;

    // uid -> name
    let mut uid_map: std::collections::HashMap<&str, &str> =
        std::collections::HashMap::new();
    for l in passwd_lines {
        let mut it = l.split_whitespace();
        if let (Some(uid), Some(name)) = (it.next(), it.next()) {
            uid_map.insert(uid, name);
        }
    }

    let mut procs: Vec<ProcStat> = Vec::new();
    for l in proc_lines {
        let line = l.trim_end_matches(['\r']);
        if line.is_empty() {
            continue;
        }
        // pid \t uid \t rss_kb \t utime \t stime \t comm
        let cols: Vec<&str> = line.splitn(6, '\t').collect();
        if cols.len() < 6 {
            continue;
        }
        let Ok(pid) = cols[0].parse::<u32>() else {
            continue;
        };
        let uid = cols[1];
        let rss_kb: u64 = cols[2].parse().unwrap_or(0);
        let utime: u64 = cols[3].parse().unwrap_or(0);
        let stime: u64 = cols[4].parse().unwrap_or(0);
        let command = cols[5].to_string();

        let cpu_secs = (utime + stime) as f32 / hertz;
        let mem_pct = if mem_total_kb > 0 {
            (rss_kb as f32 / mem_total_kb as f32) * 100.0
        } else {
            0.0
        };

        let user = uid_map
            .get(uid)
            .map(|n| n.to_string())
            .unwrap_or_else(|| uid.to_string());

        procs.push(ProcStat {
            pid,
            user,
            // BusyBox 路径下没有瞬时 %CPU；用 cpu_time_secs 排序
            cpu_percent: 0.0,
            mem_percent: mem_pct,
            cpu_time_secs: Some(cpu_secs),
            mem_rss_kb: Some(rss_kb),
            command,
        });
    }
    rank_procs_proc(procs)
}

fn rank_procs_proc(procs: Vec<ProcStat>) -> (Vec<ProcStat>, Vec<ProcStat>) {
    // BusyBox 路径下 cpu_percent 都是 0，按 cpu_time_secs 排序
    let mut top_cpu = procs.clone();
    top_cpu.sort_by(|a, b| {
        let av = a.cpu_time_secs.unwrap_or(0.0);
        let bv = b.cpu_time_secs.unwrap_or(0.0);
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
    });
    top_cpu.truncate(5);

    let mut top_mem = procs;
    top_mem.sort_by(|a, b| {
        let av = a.mem_rss_kb.unwrap_or(0);
        let bv = b.mem_rss_kb.unwrap_or(0);
        bv.cmp(&av)
    });
    top_mem.truncate(5);

    (top_cpu, top_mem)
}

fn rank_procs(procs: Vec<ProcStat>) -> (Vec<ProcStat>, Vec<ProcStat>) {
    let mut top_cpu = procs.clone();
    top_cpu.sort_by(|a, b| {
        b.cpu_percent
            .partial_cmp(&a.cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    top_cpu.truncate(5);

    let mut top_mem = procs;
    top_mem.sort_by(|a, b| {
        b.mem_percent
            .partial_cmp(&a.mem_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    top_mem.truncate(5);

    (top_cpu, top_mem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temps_dedup_and_sd() {
        let lines = vec![
            "TZ\tx86_pkg_temp\t45000",
            "HW\tcoretemp\tPackage id 0\t45000", // 与 TZ 温度相近但 label 不同，保留
            "HW\tnvme\tComposite\t38500",
            "HW\tnvme\t\t41000", // 无 label 时回退 hwmon 名
            "SD\tsda\t34",
            "SD\tsdb\t0",       // 无效读数过滤
            "TZ\tbroken\tabc",  // 解析失败跳过
        ];
        let temps = parse_temps(&lines);
        let labels: Vec<&str> = temps.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"sda"));
        assert!(labels.contains(&"Composite"));
        assert!(labels.contains(&"nvme"));
        assert!(!labels.contains(&"sdb"));
        assert!(!labels.contains(&"broken"));
        let sda = temps.iter().find(|t| t.label == "sda").unwrap();
        assert!((sda.temp_c - 34.0).abs() < 0.01);
        let nvme = temps.iter().find(|t| t.label == "Composite").unwrap();
        assert!((nvme.temp_c - 38.5).abs() < 0.01);
        // 按温度降序
        assert!(temps.windows(2).all(|w| w[0].temp_c >= w[1].temp_c));
    }

    #[test]
    fn test_parse_gpus_ascend_new_format() {
        let lines = vec![
            "+-------------------+-----------------+",
            "| npu-smi 23.0      Version: 23.0.rc2 |",
            "+===================+==================",
            "| NPU     Name      | Health          |",
            "| Chip              | Bus-Id          |",
            "+===================+==================",
            "| 0       910B4     | OK              | 68.5    45       0 / 15 |",
            "| 0                 | 0x0000          | 0       651 / 1517   1141 / 32768 |",
            "| 1       910B4     | OK              | 71.2    52       0 / 15 |",
            "| 1                 | 0x0000          | 30      800 / 1517    2000 / 32768 |",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].vendor, "ascend");
        assert_eq!(gpus[0].name, "910B4");
        assert!((gpus[0].temp_c - 45.0).abs() < 0.01);
        assert!((gpus[0].util_percent - 0.0).abs() < 0.01);
        assert_eq!(gpus[0].mem_used_mb, 1141);
        assert_eq!(gpus[0].mem_total_mb, 32768);
        assert_eq!(gpus[0].power_watts, Some(68.5));
        assert_eq!(gpus[0].health.as_deref(), Some("OK"));
        assert!((gpus[1].util_percent - 30.0).abs() < 0.01);
        assert_eq!(gpus[1].mem_used_mb, 2000);
        assert_eq!(gpus[1].power_watts, Some(71.2));
    }

    #[test]
    fn test_parse_gpus_ascend_old_format() {
        let lines = vec![
            "| 0         910             | OK              | 63.5         41               0 / 15          |",
            "| 0                         | 0x0000          | 0            736  / 32268                     |",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].name, "910");
        assert!((gpus[0].temp_c - 41.0).abs() < 0.01);
        assert_eq!(gpus[0].mem_used_mb, 736);
        assert_eq!(gpus[0].mem_total_mb, 32268);
        assert_eq!(gpus[0].power_watts, Some(63.5));
        assert_eq!(gpus[0].health.as_deref(), Some("OK"));
    }

    #[test]
    fn test_parse_gpus_ascend_25_format() {
        // npu-smi 25.2.0（910B2 实机输出）：总线号 0000:C1:00.0，显存 60465/ 65536
        let lines = vec![
            "+------------------------------------------------------------------------------------------------+",
            "| npu-smi 25.2.0                   Version: 25.2.0                                               |",
            "+---------------------------+---------------+----------------------------------------------------+",
            "| NPU   Name                | Health        | Power(W)    Temp(C)           Hugepages-Usage(page)|",
            "| Chip                      | Bus-Id        | AICore(%)   Memory-Usage(MB)  HBM-Usage(MB)        |",
            "+===========================+===============+====================================================+",
            "| 0     910B2               | OK            | 93.7        40                0    / 0             |",
            "| 0                         | 0000:C1:00.0  | 0           0    / 0          60465/ 65536         |",
            "+===========================+===============+====================================================+",
            "| 1     910B2               | OK            | 96.2        42                0    / 0             |",
            "| 0                         | 0000:01:00.0  | 0           0    / 0          60465/ 65536         |",
            "+===========================+===============+====================================================+",
            // 底部进程表：不应产生记录，也不能污染配对
            "| NPU     Chip              | Process id    | Process name             | Process memory(MB)      |",
            "| 0       0                 | 130489        | VLLMWorker_TP            | 57117                   |",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].index, 0);
        assert_eq!(gpus[1].index, 1);
        assert_eq!(gpus[0].vendor, "ascend");
        assert_eq!(gpus[0].name, "910B2");
        assert!((gpus[0].temp_c - 40.0).abs() < 0.01);
        assert!((gpus[0].util_percent - 0.0).abs() < 0.01);
        assert_eq!(gpus[0].mem_used_mb, 60465);
        assert_eq!(gpus[0].mem_total_mb, 65536);
        assert!((gpus[1].temp_c - 42.0).abs() < 0.01);
        assert_eq!(gpus[0].power_watts, Some(93.7));
        assert_eq!(gpus[1].power_watts, Some(96.2));
        assert_eq!(gpus[0].health.as_deref(), Some("OK"));
        // 底部进程表行不产生记录，健康态也不被污染
        assert_eq!(gpus[1].health.as_deref(), Some("OK"));
    }

    #[test]
    fn test_parse_gpus_nv_extended_fields() {
        // 扩展 10 列：name util used total temp fan power limit pstate
        let lines = vec!["NV\tNVIDIA GeForce RTX 4090\t68\t20305\t24564\t33\t30\t21.36\t450\tP8"];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].fan_percent, Some(30.0));
        assert_eq!(gpus[0].power_watts, Some(21.36));
        assert_eq!(gpus[0].power_limit_watts, Some(450.0));
        assert_eq!(gpus[0].perf_state.as_deref(), Some("P8"));

        // [N/A] / 空字段 → None（老驱动兜底查询只有 6 列）
        let lines = vec![
            "NV\tNVIDIA A100\t80\t1024\t40960\t66\t[N/A]\t[N/A]\t[N/A]\t[N/A]",
            "NV\tNVIDIA T4\t10\t512\t16384\t55",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 2);
        for g in &gpus {
            assert_eq!(g.fan_percent, None);
            assert_eq!(g.power_watts, None);
            assert_eq!(g.power_limit_watts, None);
            assert_eq!(g.perf_state, None);
        }
    }

    #[test]
    fn test_parse_gpus_nv_pcie_pairing() {
        // NVPCIE 行按行序与 NV 卡配对，兼容 "4"/"16" 与 "Gen4"/"x16" 两种写法
        let lines = vec![
            "NV\tRTX 4090\t0\t1\t2\t33\t30\t21\t450\tP8",
            "NV\tRTX 4090\t0\t1\t2\t33\t30\t21\t450\tP0",
            "NVPCIE\t4\t16",
            "NVPCIE\tGen5\tx8",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus[0].pcie.as_deref(), Some("Gen4 x16"));
        assert_eq!(gpus[1].pcie.as_deref(), Some("Gen5 x8"));

        // 行数不齐（PCIe 查询失败）时整组丢弃，不错配
        let lines = vec![
            "NV\tRTX 4090\t0\t1\t2\t33\t30\t21\t450\tP8",
            "NV\tRTX 4090\t0\t1\t2\t33\t30\t21\t450\tP0",
            "NVPCIE\t4\t16",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].pcie, None);
        assert_eq!(gpus[1].pcie, None);
    }

    #[test]
    fn test_parse_gpus_am_extended_fields() {
        // 扩展 10 列：name busy used total temp fan% rpm power limit
        let lines = vec!["AM\tAMD Radeon RX 7900\t33\t4294967296\t17179869184\t45000\t55\t1200\t168.5\t290"];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].fan_percent, Some(55.0));
        assert_eq!(gpus[0].fan_rpm, Some(1200));
        assert_eq!(gpus[0].power_watts, Some(168.5));
        assert_eq!(gpus[0].power_limit_watts, Some(290.0));

        // 老 6 列行仍可解析
        let lines = vec!["AM\tAMD GPU\t10\t1073741824\t8589934592\t41000"];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].fan_percent, None);
        assert_eq!(gpus[0].fan_rpm, None);
        assert_eq!(gpus[0].power_watts, None);
        assert_eq!(gpus[0].mem_total_mb, 8192);
    }

    #[test]
    fn test_parse_gpus_nv_am_unchanged() {
        let lines = vec![
            "NV\tNVIDIA A100\t80\t1024\t40960\t66",
            "AM\tAMD Radeon RX 7900\t33\t4294967296\t17179869184\t45000",
        ];
        let gpus = parse_gpus(&lines);
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].vendor, "nvidia");
        assert_eq!(gpus[0].mem_total_mb, 40960);
        assert_eq!(gpus[1].vendor, "amd");
        assert_eq!(gpus[1].mem_used_mb, 4096);
        assert_eq!(gpus[1].mem_total_mb, 16384);
        assert!((gpus[1].temp_c - 45.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod tests2 {
    use super::*;

    #[test]
    fn test_parse_cpu_model() {
        assert_eq!(
            parse_cpu_model(&[" Intel(R) Xeon(R) Gold 6248R CPU @ 3.00GHz", ""]),
            "Intel(R) Xeon(R) Gold 6248R CPU @ 3.00GHz"
        );
        assert_eq!(parse_cpu_model(&["", " "]), "");
    }

    #[test]
    fn test_parse_cpu_stat() {
        // user nice sys idle iowait irq softirq steal + guest*2（guest 已计入 user，不计入 total）
        let t =
            parse_cpu_stat(&["cpu  100 10 200 600 40 5 5 5 3 3", "cpu0 50 5 100 300 20 2 2 2"])
                .unwrap();
        assert_eq!(t.total, 100 + 10 + 200 + 600 + 40 + 5 + 5 + 5);
        assert_eq!(t.idle, 600 + 40);
        // 单核行不参与；字段不足（缺 idle）丢弃
        assert_eq!(parse_cpu_stat(&["cpu0 1 2 3 4"]), None);
        assert_eq!(parse_cpu_stat(&["cpu  1 2 3"]), None);
        assert_eq!(parse_cpu_stat(&[]), None);
    }

    #[test]
    fn test_cpu_pct_between() {
        let prev = CpuTicks {
            total: 1000,
            idle: 900,
        };
        // 1 秒内 total +100、idle +90 → 10%
        let cur = CpuTicks {
            total: 1100,
            idle: 990,
        };
        assert!((cpu_pct_between(prev, cur, 1.0).unwrap() - 10.0).abs() < 0.01);
        // 全忙：idle 不涨
        let busy = CpuTicks {
            total: 1100,
            idle: 900,
        };
        assert_eq!(cpu_pct_between(prev, busy, 1.0), Some(100.0));
        // 间隔过短 / 过长不可信
        assert_eq!(cpu_pct_between(prev, busy, 0.1), None);
        assert_eq!(cpu_pct_between(prev, busy, 30.0), None);
        // 计数回绕（主机重启）
        let rebooted = CpuTicks { total: 10, idle: 9 };
        assert_eq!(cpu_pct_between(prev, rebooted, 1.0), None);
    }

    #[test]
    fn test_parse_disks_filters_pseudo_mounts() {
        let lines = vec![
            "/dev/sda1 1024 512 512 50% /",
            "efivarfs 56 27 27 50% /sys/firmware/efi/efivars",
            "/dev/nvme0n1p1 2048 1024 1024 50% /boot/efi",
            // 前缀与 /proc 相近但非伪目录，保留
            "/dev/sdb1 4096 0 4096 0% /procedures",
        ];
        let disks = parse_disks(&lines);
        let mounts: Vec<&str> = disks.iter().map(|d| d.mount.as_str()).collect();
        assert!(mounts.contains(&"/"));
        assert!(mounts.contains(&"/boot/efi"));
        assert!(mounts.contains(&"/procedures"));
        assert!(!mounts.iter().any(|m| m.contains("efivars")));
        // / 排第一
        assert_eq!(disks[0].mount, "/");
    }

    #[test]
    fn test_attach_disk_models_mmc() {
        let models = vec![("mmcblk0".to_string(), "SD SDSC".to_string())];
        let mut disks = vec![DiskUsage {
            filesystem: "/dev/mmcblk0p1".into(),
            mount: "/".into(),
            total_bytes: 1,
            used_bytes: 0,
            model: None,
        }];
        attach_disk_models(&mut disks, models);
        assert_eq!(disks[0].model.as_deref(), Some("SD SDSC"));
    }

    #[test]
    fn test_parse_dmi_mem() {
        let (t, sp, n) = parse_dmi_mem(&["DIM\t8\tDDR4\t3200 MT/s"]);
        assert_eq!((t.as_str(), sp.as_str(), n), ("DDR4", "3200 MT/s", 8));
        let (t, sp, n) = parse_dmi_mem(&["DIM\t0\t\t", "other"]);
        assert_eq!((t.as_str(), sp.as_str(), n), ("", "", 0));
    }

    #[test]
    fn test_attach_disk_models() {
        let models = vec![
            ("sda".to_string(), "SAMSUNG MZQL21T9".to_string()),
            ("nvme0n1".to_string(), "INTEL SSDPE2KX010T8".to_string()),
        ];
        let mut disks = vec![
            DiskUsage {
                filesystem: "/dev/sda1".into(),
                mount: "/".into(),
                total_bytes: 1,
                used_bytes: 0,
                model: None,
            },
            DiskUsage {
                filesystem: "/dev/nvme0n1p2".into(),
                mount: "/data".into(),
                total_bytes: 1,
                used_bytes: 0,
                model: None,
            },
            DiskUsage {
                filesystem: "/dev/mapper/vg-root".into(),
                mount: "/x".into(),
                total_bytes: 1,
                used_bytes: 0,
                model: None,
            },
        ];
        attach_disk_models(&mut disks, models);
        assert_eq!(disks[0].model.as_deref(), Some("SAMSUNG MZQL21T9"));
        assert_eq!(disks[1].model.as_deref(), Some("INTEL SSDPE2KX010T8"));
        // LVM 等非物理设备不误挂型号
        assert_eq!(disks[2].model, None);
    }

    /// macOS 采集段的端到端解析：Darwin 分支伪装出的行格式要能走通 Linux 解析路径
    #[test]
    fn test_parse_macos_sections() {
        let text = "\
===SECTION:hostname===
Mac-mini.local
===SECTION:os===
PRETTY_NAME=\"macOS 27.0.1\"
===SECTION:kernel===
Darwin 27.0.0
===SECTION:arch===
arm64
===SECTION:uptime===
1310096
===SECTION:stat===
cpu 324 0 9675 2222
===SECTION:cores===
10
===SECTION:mem===
MemTotal: 16777216 kB
MemAvailable: 7312032 kB
SwapTotal: 3145728 kB
SwapFree: 1491394 kB
===SECTION:net===


en0: 11040591480 0 0 0 0 0 0 0 17299745655 0 0 0 0 0 0 0
===SECTION:ps===
  PID USER              %CPU %MEM COMM
    1 root               0.1  0.1 /sbin/launchd
  539 root               0.1  0.4 /usr/libexec/logd
===SECTION:cpuinfo===
Apple M4
===SECTION:end===
";
        let (info, ticks) = parse(text);
        assert_eq!(info.hostname, "Mac-mini.local");
        assert_eq!(info.os_pretty, "macOS 27.0.1");
        assert_eq!(info.kernel, "Darwin 27.0.0");
        assert_eq!(info.arch, "arm64");
        assert_eq!(info.uptime_secs, 1310096);
        // 伪节拍行：user+sys+idle
        let t = ticks.unwrap();
        assert_eq!(t.total, 12221);
        assert_eq!(t.idle, 2222);
        assert_eq!(info.cpu_cores, 10);
        assert_eq!(info.cpu_model, "Apple M4");
        assert_eq!(info.mem_total_kb, 16777216);
        assert_eq!(info.mem_used_kb, 16777216 - 7312032);
        assert_eq!(info.swap_total_kb, 3145728);
        assert_eq!(info.swap_used_kb, 3145728 - 1491394);
        // 首两行空行占位被 skip，只落真实网卡
        assert_eq!(info.nics.len(), 1);
        assert_eq!(info.nics[0].name, "en0");
        assert_eq!(info.nics[0].rx_bytes, 11040591480);
        assert_eq!(info.nics[0].tx_bytes, 17299745655);
        assert_eq!(info.net_rx_bytes, 11040591480);
        // procps 路径解析 ps：表头跳过，comm 含完整路径
        assert_eq!(info.top_cpu.len(), 2);
        assert!(info.top_cpu.iter().any(|p| p.pid == 1));
        assert!(info.top_mem.iter().any(|p| p.command == "/usr/libexec/logd"));
        // 差分：两帧 total 增 1000、idle 增 222 → busy ≈ 77.8%
        let prev = CpuTicks { total: 11221, idle: 2000 };
        assert!((cpu_pct_between(prev, t, 1.5).unwrap() - 77.8).abs() < 0.1);
    }
}
