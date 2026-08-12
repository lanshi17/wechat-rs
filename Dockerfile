# ── 构建阶段 ─────────────────────────────────────────────────────────────────
FROM rust:1.88-bookworm AS builder

WORKDIR /build

# 复制依赖文件，利用 Docker 缓存
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

# 构建 release 版本
RUN cargo build --release

# ── 运行阶段 ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/wechat-rs /usr/local/bin/wechat-rs

EXPOSE 3000

ENV CONFIG_PATH=/etc/wechat-rs/config.toml

CMD ["wechat-rs"]