FROM debian:bookworm-slim
ENV DEBIAN_FRONTEND=noninteractive
ENV LC_ALL=C.UTF-8
ENV LANG=C.UTF-8

ARG CONTAINER_USER=vscode

RUN apt update && \
    apt install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    curl \
    gcc \
    gdb-multiarch \
    git \
    libc6-dev \
    libssl-dev \
    openocd \
    pkg-config \
    udev \
    usbutils && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

# Set users
RUN adduser --disabled-password --gecos "" ${CONTAINER_USER}
ENV HOME=/home/${CONTAINER_USER} \
    USER=${CONTAINER_USER} \
    CARGO_HOME=/home/${CONTAINER_USER}/.cargo \
    PATH=/home/${CONTAINER_USER}/.cargo/bin:"$PATH"

# Install rustup
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    --no-modify-path

RUN rustup set profile minimal && \
    rustup default nightly-2025-05-09 && \
    rustup component add clippy llvm-tools rustfmt && \
    rustup target add thumbv8m.main-none-eabihf

RUN cargo install \
    cargo-binstall \
    cargo-generate \
    flip-link

RUN cargo binstall probe-rs-tools -y

RUN chown -R ${CONTAINER_USER}:${CONTAINER_USER} /home/${CONTAINER_USER}/.rustup

RUN chown -R ${CONTAINER_USER}:${CONTAINER_USER} ${CARGO_HOME}/ \
    && chmod -R +w ${HOME}/.cargo

CMD ["/bin/bash"]
