# ── 构建阶段 ─────────────────────────────────────────────────────────────────
FROM rust:1.82-bookworm AS builder

WORKDIR /build

# 先复制依赖文件，利用 Docker 缓存
COPY Cargo.toml Cargo.lock ./
RUN cargo fetch --locked

# 复制源代码
COPY src/ src/

# 构建 release 版本
RUN cargo build --release --locked && \
    strip target/release/wechat-rs

# ── 运行阶段 ─────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/wechat-rs /usr/local/bin/wechat-rs

EXPOSE 3000

# Railway/Docker 会通过 PORT 环境变量注入端口
ENV CONFIG_PATH=/etc/wechat-rs/config.toml

CMD ["wechat-rs"]
