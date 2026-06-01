FROM registry.cn-hangzhou.aliyuncs.com/davepaine/wechat-rs:latest
COPY target/release/wechat-rs /usr/local/bin/wechat-rs
EXPOSE 3000
ENV CONFIG_PATH=/etc/wechat-rs/config.toml
CMD ["wechat-rs"]
