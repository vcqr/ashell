/**
 * macOS 二进制签名标识钉死（挂在 tauri.conf.json 的 beforeBundleCommand，也可独立执行）。
 *
 * 背景：macOS 的本地网络等 TCC/隐私授权按「二进制签名标识」逐条匹配放行记录，而未
 * 显式签名的产物标识由链接器自动生成且随机（rust 主程序为 ashell-<hash>，bun 编译的
 * sidecar 为 a.out）。每次重新构建标识都变，旧授权记录随之失效，系统又因 bundle id
 * 已有记录不再弹窗——最终出现「设置里已允许、连接仍被拒」的死锁。
 *
 * 行为：把主程序 ashell、ashell-server 与 sidecar app-ai 以 adhoc 签名钉死为标识
 * com.vcqr.ashell（与 tauri.conf.json 的 identifier 一致，复用同一条放行记录）。
 * 标识已正确的二进制直接跳过，幂等可重复执行；非 macOS 平台直接放行。
 *
 * 注意：tauri dev 不走打包钩子，cargo 重新编译后如需本地网络授权，
 * 手动执行 npm run sign:mac 再重启 dev 即可。
 */
import { spawnSync } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const identifier = "com.vcqr.ashell";
const tag = "[sign-macos]";

if (process.platform !== "darwin") process.exit(0);

/** Mach-O 魔数：64 位小/大端 + FAT 通用二进制 */
const MACHO_MAGICS = [
  Buffer.from([0xcf, 0xfa, 0xed, 0xfe]),
  Buffer.from([0xfe, 0xed, 0xfa, 0xcf]),
  Buffer.from([0xca, 0xfe, 0xba, 0xbe]),
];

function isMachO(file) {
  const fd = fs.openSync(file, "r");
  try {
    const head = Buffer.alloc(4);
    fs.readSync(fd, head, 0, 4, 0);
    return MACHO_MAGICS.some((m) => head.equals(m));
  } finally {
    fs.closeSync(fd);
  }
}

/** 当前签名标识；未签名或读取失败返回 null */
function currentIdentifier(file) {
  const res = spawnSync("codesign", ["-dv", file], { encoding: "utf8" });
  return /^Identifier=(.+)$/m.exec(res.stderr)?.[1]?.trim() ?? null;
}

/** 待签名候选：target 下的 rust 产物（含 --target 三元组目录）+ binaries 下的 sidecar */
function candidates() {
  const list = [];
  const bins = ["ashell", "ashell-server"];
  const targetsDir = path.join(root, "src-tauri", "target");
  for (const entry of fs.existsSync(targetsDir) ? fs.readdirSync(targetsDir) : []) {
    const dir = path.join(targetsDir, entry);
    if (entry === "debug" || entry === "release") {
      for (const bin of bins) list.push(path.join(dir, bin));
    } else if (entry.endsWith("-apple-darwin")) {
      for (const profile of ["debug", "release"]) {
        for (const bin of bins) list.push(path.join(dir, profile, bin));
      }
    }
  }

  const binariesDir = path.join(root, "src-tauri", "binaries");
  for (const name of fs.existsSync(binariesDir) ? fs.readdirSync(binariesDir) : []) {
    if (name.startsWith("app-ai")) list.push(path.join(binariesDir, name));
  }
  return [...new Set(list)];
}

let failures = 0;
for (const file of candidates()) {
  if (!fs.existsSync(file) || !isMachO(file)) continue;
  const rel = path.relative(root, file);
  if (currentIdentifier(file) === identifier) {
    console.log(`${tag} ${rel} 标识已正确，跳过`);
    continue;
  }
  const res = spawnSync("codesign", ["--force", "--sign", "-", "--identifier", identifier, file]);
  if (res.status !== 0) {
    console.error(`${tag} 签名失败：${rel}`);
    process.stderr.write(res.stderr);
    failures += 1;
    continue;
  }
  console.log(`${tag} 已钉死签名标识 ${identifier}：${rel}`);
}

if (failures > 0) process.exit(1);
console.log(`${tag} 完成`);
