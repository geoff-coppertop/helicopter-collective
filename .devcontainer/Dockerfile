FROM rust:1.86-slim

ARG CONTAINER_USER=vscode

RUN apt update && \
    apt install -y --no-install-recommends \
        gdb-multiarch \
        git \
        libssl-dev \
        openocd \
        pkg-config \
        udev && \
    apt clean && \
    rm -rf /var/lib/apt/lists/*

RUN rustup set profile minimal && \
    rustup default 1.86.0 && \
    rustup component add clippy llvm-tools rustfmt && \
    rustup target add thumbv7em-none-eabi && \
    rustup target add thumbv7em-none-eabihf

RUN adduser --disabled-password --gecos "" ${CONTAINER_USER}

ENV USER=${CONTAINER_USER}
USER ${CONTAINER_USER}

CMD ["/bin/bash"]
