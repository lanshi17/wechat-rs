FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY target/release/wechat-rs /usr/local/bin/wechat-rs
EXPOSE 3000
CMD ["wechat-rs"]
