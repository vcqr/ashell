#!/usr/bin/env bash
# 前端热同步：构建 dist 并同步到 docker/dist（compose 只读挂载到容器 /app/dist）。
# 同步完成刷新浏览器即生效，无需重建镜像、无需重启容器。
#
# 注意：Rust / sidecar 代码变更仍需完整重建：./scripts/build-server-image.sh

set -euo pipefail
cd "$(dirname "$0")/.."

npm run build
rm -rf docker/dist
cp -R dist docker/dist
echo "前端已同步，刷新浏览器生效"
