FROM rust:1.98.0-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
      libasound2-dev libssl-dev pkg-config python3 python3-venv ripgrep git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --shell /bin/bash cortex \
    && mkdir -p /home/cortex/.cargo /home/cortex/.rustup \
    && cp -a /usr/local/rustup/. /home/cortex/.rustup/ \
    && chown -R cortex:cortex /home/cortex
ENV RUSTUP_HOME=/home/cortex/.rustup \
    CARGO_HOME=/home/cortex/.cargo
ENV PATH=/usr/local/cargo/bin:/home/cortex/.cargo/bin:${PATH}
USER cortex
WORKDIR /workspaces/cli
