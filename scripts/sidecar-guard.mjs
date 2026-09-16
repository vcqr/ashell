/**
 * app-ai sidecar 二进制新鲜度守卫（挂在 tauri.conf.json 的 beforeDevCommand）。
 *
 * 背景：sidecar-ai 是 bun compile 产物、不进 git，而 tauri dev 原本的
 * beforeDevCommand 只构建前端。克隆仓库、拉取代码或改动 sidecar-ai 源码后，
 * src-tauri/binaries 下的旧二进制不会自动重建——宿主以 --serve 拉起旧版
 * app-ai 时它不认识 daemon 协议，打印「程序启动失败，缺少关键信息」即退出，
 * 只留下 [DAEMON] non-frame stdout dropped 这类难排查的 WARN。
 *
 * 行为：源码 mtime 比二进制新（或二进制缺失）时执行 npm run sidecar:build，
 * 否则直接放行（无感知，~百毫秒开销）。
 */
import { execSync, spawnSync } from "child_process";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const srcDir = path.join(root, "sidecar-ai", "src");

/** 递归取目录内最新 mtime（毫秒）；空/缺失返回 0 */
function newestMtime(dir) {
  let newest = 0;
  for (const entry of fs.existsSync(dir) ? fs.readdirSync(dir, { withFileTypes: true }) : []) {
    const p = path.join(dir, entry.name);
    const m = entry.isDirectory() ? newestMtime(p) : fs.statSync(p).mtimeMs;
    if (m > newest) newest = m;
  }
  return newest;
}

const rustInfo = execSync("rustc -vV").toString();
const triple = /host: (\S+)/.exec(rustInfo)?.[1];
if (!triple) {
  console.error("[sidecar-guard] 无法确定 rustc host 三元组，跳过检查");
  process.exit(0);
}
const ext = process.platform === "win32" ? ".exe" : "";
const binary = path.join(root, "src-tauri", "binaries", `app-ai-${triple}${ext}`);

// 源码 + 包清单（依赖变更也会影响产物）；node_modules 不参与
const srcNewest = Math.max(
  newestMtime(srcDir),
  fs.statSync(path.join(root, "sidecar-ai", "package.json")).mtimeMs,
);

const binMtime = fs.existsSync(binary) ? fs.statSync(binary).mtimeMs : 0;
if (binMtime >= srcNewest) {
  console.log("[sidecar-guard] app-ai 二进制已是最新，跳过重建");
  process.exit(0);
}

console.log(
  `[sidecar-guard] app-ai 二进制${binMtime === 0 ? "缺失" : "过旧（比 sidecar-ai 源码旧）"}，重建中...`,
);
const res = spawnSync("npm run sidecar:build", {
  cwd: root,
  stdio: "inherit",
  shell: true,
});
process.exit(res.status ?? 1);
