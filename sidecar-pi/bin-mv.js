import { execSync } from "child_process";
import fs from "fs";

const ext = process.platform === "win32" ? ".exe" : "";

const rustInfo = execSync("rustc -vV");
const targetTriple = /host: (\S+)/g.exec(rustInfo)[1];
if (!targetTriple) {
  console.error("无法确定平台目标三元组");
}
fs.renameSync(`app-pi${ext}`, `../src-tauri/binaries/app-pi-${targetTriple}${ext}`);
