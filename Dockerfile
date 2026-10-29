# ── 构建阶段 ─────────────────────────────────────────────────────────────────
# 用 1-bookworm 跟踪最新 Rust 1.x，避免 Cargo.lock 中依赖的 MSRV 超过工具链版本
FROM rust:1-bookworm AS builder

WORKDIR /build

# ring 等 crate 需要 C 编译器（官方 rust 镜像自带 gcc），pkg-config 以备不时之需
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# 先复制依赖描述，利用 Docker 层缓存
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
ENV CARGO_INCREMENTAL=0 CARGO_NET_RETRY=10 CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=off

# --locked：严格按 Cargo.lock 构建，保证可复现，避免静默升级依赖。
# JOBS=2 + 关闭 LTO：把构建内存峰值压低（并行 rustc + LTO 链接是 OOM 重灾区，
# 小内存构建机上构建进程被杀时 Railway 只显示笼统的 build 失败）；二进制稍大，无功能影响。
RUN cargo build --release --locked

# ── 运行阶段 ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    mkdir -p /etc/wechat-rs

COPY --from=builder /build/target/release/wechat-rs /usr/local/bin/wechat-rs

# 实际监听端口由 $PORT 环境变量决定（Railway 自动注入，代码已支持覆盖），此处仅为文档声明
EXPOSE 8080

ENV CONFIG_PATH=/etc/wechat-rs/config.toml

CMD ["/usr/local/bin/wechat-rs"]
