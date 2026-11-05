#!/usr/bin/env bash
# sudo 密码提示误报探针：从真实源码抽取检测函数，编译运行构造用例。
#
# 为什么不直接把函数抄进脚本：抄写会与源码漂移，下次改过检测逻辑就测不准了。
# 这里按函数名从 src-tauri/src/handlers/terminal.rs 里逐个抽取原文，
# 再与 scripts/sudo-fp-cases.rs 拼成临时 crate 编译 —— 测的永远是当前源码。
#
# 用法：
#   ./scripts/sudo-fp-probe.sh              # 跑全部用例，打印对照表
#   ./scripts/sudo-fp-probe.sh --strict     # 同时打印"收紧后"的判定，看修复收益
#   ./scripts/sudo-fp-probe.sh --only=B12   # 只跑 id 匹配 B12 的用例
#
# 退出码：0 = 全部用例符合预期；1 = 有用例与预期不符（说明检测行为变了）

set -euo pipefail
cd "$(dirname "$0")/.."

SRC="src-tauri/src/handlers/terminal.rs"
CASES="scripts/sudo-fp-cases.rs"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

if [[ ! -f "$SRC" || ! -f "$CASES" ]]; then
  echo "缺少 $SRC 或 $CASES" >&2
  exit 2
fi

# 抽取单个函数原文：从 "^fn NAME" 起，到第一个独占一行的 "}" 止。
# 用法：extract_fn <函数名> <输出文件>
extract_fn() {
  awk -v want="$1" '
    $0 ~ "^fn " want "\\(" { grab = 1 }
    grab { print }
    grab && /^}$/ { exit }
  ' "$SRC" > "$2"
}

FNS=(
  sanitize_prompt_text
  tail_chars
  last_input_line
  last_input_requests_elevation
  input_line_echoes_colon
  detect_sudo_password_prompt
)

{
  echo "// 由 scripts/sudo-fp-probe.sh 从 $SRC 自动抽取，请勿手改"
  echo "#![allow(dead_code)]"
  for fn in "${FNS[@]}"; do
    extract_fn "$fn" "$WORK/fn.rs"
    if [[ ! -s "$WORK/fn.rs" ]]; then
      echo "抽取失败：找不到函数 $fn（源码结构可能变了）" >&2
      exit 2
    fi
    cat "$WORK/fn.rs"
    echo
  done
  cat "$CASES"
} > "$WORK/probe.rs"

rustc -O --edition 2021 -o "$WORK/probe" "$WORK/probe.rs" 2>&1 | sed 's/^/  /'
exec "$WORK/probe" "$@"
