# syntax=docker/dockerfile:1
FROM docker.io/library/debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8

# OS-level packages. User creation, Rust toolchain, and embedded cargo tools
# all come from devcontainer features (common-utils,
# ghcr.io/devcontainers/features/rust,
# ghcr.io/geoff-coppertop/devcontainer-features/embedded-rust-tools).
RUN apt update && \
    apt install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    gcc \
    gdb-multiarch \
    libc6-dev \
    libssl-dev \
    openocd \
    pkg-config \
    udev \
    usbutils && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

CMD ["/bin/bash"]
