//! 主机系统信息采集
//!
//! 通过 `Session::execute` 在已建立的 SSH 会话上执行一段复合 shell，把多块信息一次性
//! 拉回来再在 Rust 侧解析。各段以 `===SECTION:<name>===` 分隔，单段内部按 Linux 习惯
//! 逐行处理。任意一段解析失败时该字段降级为默认值（数字 0 / 字符串空），不抛错，让前
//! 端能继续显示其它字段。

use serde::Serialize;

use crate::errors::AppResult;
use crate::service::ssh as ssh_svc;

#[derive(Debug, Serialize)]
pub struct DiskUsage {
    pub filesystem: String,
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
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
    /// "nvidia" / "amd"
    pub vendor: String,
    /// 型号名；拿不到时用 "AMD GPU" 兼底
    pub name: String,
    /// 0 - 100；不支持读取时为 0
    pub util_percent: f32,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub temp_c: f32,
}

#[derive(Debug, Serialize)]
pub struct SysInfo {
    pub hostname: String,
    pub os_pretty: String,
    pub kernel: String,
    pub arch: String,
    pub uptime_secs: u64,
    pub cpu_percent: f32,
    pub cpu_cores: u32,
    pub mem_total_kb: u64,
    pub mem_used_kb: u64,
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
echo '===SECTION:hostname==='
hostname 2>/dev/null
echo '===SECTION:os==='
cat /etc/os-release 2>/dev/null
echo '===SECTION:kernel==='
uname -sr 2>/dev/null
echo '===SECTION:arch==='
uname -m 2>/dev/null
echo '===SECTION:uptime==='
awk '{print $1}' /proc/uptime 2>/dev/null
echo '===SECTION:cpu==='
LC_ALL=C top -bn1 2>/dev/null | grep -i '^%\?Cpu'
echo '===SECTION:cores==='
nproc 2>/dev/null
echo '===SECTION:mem==='
cat /proc/meminfo 2>/dev/null
echo '===SECTION:disk==='
LC_ALL=C df -Pk 2>/dev/null | grep -Ev '^(tmpfs|devtmpfs|overlay|aufs|squashfs|udev|run|none)\b' | grep -Ev '^Filesystem\b'
echo '===SECTION:net==='
cat /proc/net/dev 2>/dev/null
echo '===SECTION:ps==='
LC_ALL=C ps -eo pid,user,pcpu,pmem,comm 2>/dev/null
echo '===SECTION:hertz==='
getconf CLK_TCK 2>/dev/null
echo '===SECTION:proc_uid_name==='
# /etc/passwd 仅取 name:uid，给 /proc 路径用来回填用户名
awk -F: '{print $3 " " $1}' /etc/passwd 2>/dev/null
echo '===SECTION:proc==='
# 遍历 /proc/[0-9]*：每行输出
#   pid <TAB> uid <TAB> rss_kb <TAB> utime_jiffies <TAB> stime_jiffies <TAB> comm
# 兼容 BusyBox awk：用 getline < file 读；comm 取 stat 里第一对 () 之间内容。
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
# NVIDIA：nvidia-smi 一次拿全利用率/显存/温度
if command -v nvidia-smi >/dev/null 2>&1; then
  nvidia-smi --query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu --format=csv,noheader,nounits 2>/dev/null |
  while IFS=, read -r n u mu mt tp; do
    n=$(printf '%s' "$n" | sed 's/^ *//; s/ *$//')
    printf 'NV\t%s\t%s\t%s\t%s\t%s\n' "$n" "$u" "$mu" "$mt" "$tp"
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
  addr=$(basename "$(readlink -f "$d/device")" 2>/dev/null)
  name=$(lspci -s "$addr" 2>/dev/null | sed 's/^.*: //')
  [ -n "$name" ] || name="AMD GPU"
  printf 'AM\t%s\t%s\t%s\t%s\t%s\n' "$name" "$busy" "$vu" "$vt" "$te"
done
# 华为昇腾 NPU：npu-smi 表格原样输出，由 Rust 侧解析
# npu-smi 常装在 /usr/local/sbin，非交互 SSH PATH 可能不含该目录，补绝对路径兜底
NPU_SM=$(command -v npu-smi 2>/dev/null || ls /usr/local/sbin/npu-smi /usr/sbin/npu-smi 2>/dev/null | head -n 1)
if [ -n "$NPU_SM" ] && [ -x "$NPU_SM" ]; then
  "$NPU_SM" info 2>/dev/null
fi
echo '===SECTION:end==='
"#;

pub async fn collect(sid: &str) -> AppResult<SysInfo> {
    let session = ssh_svc::get_client(sid).await?;
    let result = session.execute(SCRIPT).await?;
    Ok(parse(&result.stdout))
}

fn parse(text: &str) -> SysInfo {
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

    let cpu_percent = parse_cpu(&take("cpu"));
    let cpu_cores = take("cores")
        .iter()
        .find_map(|l| l.trim().parse::<u32>().ok())
        .unwrap_or(0);

    let (mem_total_kb, mem_used_kb, swap_total_kb, swap_used_kb) = parse_mem(&take("mem"));
    let disks = parse_disks(&take("disk"));
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

    SysInfo {
        hostname,
        os_pretty,
        kernel,
        arch,
        uptime_secs,
        cpu_percent,
        cpu_cores,
        mem_total_kb,
        mem_used_kb,
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
    }
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

/// 从 top 输出中解析 idle 字段，返回 100 - idle 作为整体 CPU 占用率
fn parse_cpu(lines: &[&str]) -> f32 {
    for l in lines {
        let lower = l.to_ascii_lowercase();
        if !lower.contains("cpu") {
            continue;
        }
        // 典型格式：%Cpu(s):  3.1 us,  1.2 sy,  0.0 ni, 95.5 id, ...
        // 或：     Cpu(s):  3.1%us,  1.2%sy,  ...,  95.5%id, ...
        // 在所有 token 中找 "id" 前面的数字
        let tokens: Vec<&str> = l
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        // 优先按顺序两 token 形式：value, "id"
        for i in 0..tokens.len() {
            if tokens[i].eq_ignore_ascii_case("id") {
                if i > 0 {
                    if let Ok(idle) = tokens[i - 1].trim_end_matches('%').parse::<f32>() {
                        return clamp_pct(100.0 - idle);
                    }
                }
            }
        }
        // 兼容紧贴形式：95.5%id 或 95.5id
        for t in &tokens {
            let lower = t.to_ascii_lowercase();
            if lower.ends_with("id") || lower.ends_with("%id") {
                let trimmed = lower.trim_end_matches("id").trim_end_matches('%');
                if let Ok(idle) = trimmed.parse::<f32>() {
                    return clamp_pct(100.0 - idle);
                }
            }
        }
    }
    0.0
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

/// 解析 GPU 段：每行 `NV\t<name>\t<util%>\t<used MiB>\t<total MiB>\t<°C>`
/// 或 `AM\t<name>\t<busy%>\t<used bytes>\t<total bytes>\t<毫摄氏度>`。
/// 字段可能为 "[N/A]" / 空，宽松解析，取不到置 0。
fn parse_gpus(lines: &[&str]) -> Vec<GpuInfo> {
    let mut out = Vec::new();
    // 昇腾 NPU 行与 Chip 行分两行输出，先用 pending 缓存配对
    let mut ascend_pending: Vec<(String, f32)> = Vec::new();
    for l in lines {
        // npu-smi 表格行（含 '|'，非制表符格式）
        if l.contains('|') {
            if let Some(gpu) = ascend_table_line(l, &mut ascend_pending) {
                out.push(gpu);
            }
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
        let gpu = match cols[0] {
            "NV" => {
                let temp = f32_of(cols[5]);
                GpuInfo {
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
                }
            }
            _ => continue,
        };
        out.push(gpu);
    }
    out
}

/// 解析 npu-smi info 表格行。
/// NPU 行：`| 0  910B4 | OK | 68.5  45 ... |` → 记下（型号, 温度）待配对；
/// Chip 行（含 0x 总线号）：`| 0 | 0x0000 | 0 | 651 / 1517 ... |` → 取 AICore%
/// 与显存（取最后一个 used / total 对，多芯片卡上即 HBM），与前一条 NPU 行合成一条记录。
/// 老版本（如 20.x）一行格里塞多个字段，拆 '|' 后再按空白二次切分统一处理。
fn ascend_table_line(l: &str, pending: &mut Vec<(String, f32)>) -> Option<GpuInfo> {
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
    let bus_idx = tokens
        .iter()
        .position(|tk| tk.len() > 2 && tk.starts_with("0x"));
    match bus_idx {
        Some(ix) => {
            let util = tokens
                .get(ix + 1)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            let (mem_used, mem_total) = match tokens.iter().rposition(|tk| *tk == "/") {
                Some(j) if j >= 1 && j + 1 < tokens.len() => (
                    tokens[j - 1].parse::<u64>().unwrap_or(0),
                    tokens[j + 1].parse::<u64>().unwrap_or(0),
                ),
                _ => (0, 0),
            };
            let (name, temp_c) = pending
                .pop()
                .unwrap_or_else(|| ("Ascend NPU".to_string(), 0.0));
            Some(GpuInfo {
                vendor: "ascend".into(),
                name,
                util_percent: clamp_pct(util),
                mem_used_mb: mem_used,
                mem_total_mb: mem_total,
                temp_c: temp_c.clamp(0.0, 150.0),
            })
        }
        None => {
            // NPU 行：tokens = [索引, 型号, 健康态, 功率, 温度, ...]
            let name = tokens[1].to_string();
            let temp_c = tokens
                .get(4)
                .and_then(|s| s.parse::<f32>().ok())
                .unwrap_or(0.0);
            pending.push((name, temp_c));
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
        });
    }
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
        assert!((gpus[1].util_percent - 30.0).abs() < 0.01);
        assert_eq!(gpus[1].mem_used_mb, 2000);
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
