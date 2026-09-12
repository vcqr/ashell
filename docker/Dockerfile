# AShell Web Server 运行时镜像（极简 COPY 式）
#
# 前置：先运行 scripts/build-server-image.sh 交叉编译二进制并组装本目录，
#       然后docker build / docker compose build 即可（秒级，无容器内编译）。
#
# 交叉编译（本机执行，产物为 musl 静态链接，alpine 直接可跑）：
#   cd src-tauri && cross build --release --bin ashell-server \
#       --no-default-features --target aarch64-unknown-linux-musl

FROM alpine:latest

# 静态二进制无额外 so 依赖；
# - ca-certificates：S3 备份 / AI 模型列表等 HTTPS 出站
# - bash + terminfo：容器内"新建本地终端"可用（busybox ash 之外的真实 shell，
#   以及 xterm-256color 等 TERM 条目，供容器内 TUI 程序正常渲染）
# /data 预创建并归属 ashell：首次挂载命名卷时继承该属主，非 root 可写
RUN apk add --no-cache ca-certificates bash ncurses-terminfo-base \
    && adduser -D -u 1000 ashell \
    && mkdir -p /data/.ashell \
    && chown -R ashell:ashell /data

# auto 模式首选 shell（存在性校验失败时后端会沿候选链回退）
ENV SHELL=/bin/bash

COPY ashell-server /app/ashell-server
COPY dist /app/dist

# HOME 指向挂载点父目录：app_dir() = $HOME/.ashell = /data/.ashell，
# compose 把宿主 ~/.ashell 挂到 /data/.ashell，容器内外共用同一数据目录。
# 容器无钥匙串/DBus：加密密钥即挂载目录下的 secret.key（首次自动生成，务必备份）。
ENV HOME=/data \
    ASHELL_FORCE_KEY_FILE=1 \
    RUST_LOG=info

WORKDIR /app
VOLUME ["/data"]
EXPOSE 8090
USER ashell
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:8090/health || exit 1
ENTRYPOINT ["/app/ashell-server"]
CMD ["--bind", "0.0.0.0:8090", "--dist", "/app/dist"]
