# syntax=docker/dockerfile:1
FROM mcr.microsoft.com/devcontainers/base:debian
ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8

# OS-level packages. The container user is provided by the base image
# (devcontainers/base ships `vscode` at UID 1000). Rust toolchain and
# embedded cargo tools come from devcontainer features
# (ghcr.io/devcontainers/features/rust,
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
