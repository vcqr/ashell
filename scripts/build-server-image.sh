#!/usr/bin/env bash
# 交叉编译 ashell-server（Linux musl 静态二进制）并组装运行时镜像。
#
# 产物布局：
#   docker/ashell-server   musl 静态二进制
#   docker/dist            前端构建产物
#   docker/Dockerfile      极简 COPY 式镜像
#
# 用法：
#   ./scripts/build-server-image.sh                      # arm64（Apple Silicon / arm 服务器）
#   TARGET=x86_64-unknown-linux-musl ./scripts/build-server-image.sh   # x86_64 服务器
#
# 首次依赖 cross 工具（已装可跳过）：
#   cargo install --git https://github.com/cross-rs/cross --locked cross

set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${TARGET:-aarch64-unknown-linux-musl}"

echo "==> 1/4 前端构建（dist/）"
npm run build

echo "==> 2/4 交叉编译 ${TARGET}（musl 静态链接）"
cd src-tauri
cross build --release --bin ashell-server --no-default-features --target "$TARGET"
cd ..

echo "==> 3/4 组装 docker/ 构建上下文"
mkdir -p docker
cp "src-tauri/target/$TARGET/release/ashell-server" docker/ashell-server
rm -rf docker/dist
cp -R dist docker/dist

echo "==> 4/4 构建镜像"
docker build -f docker/Dockerfile -t ashell-server:latest docker/

echo "完成：ashell-server:latest"
echo "运行：docker compose up -d"
