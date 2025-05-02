#!/bin/sh

apt update && \
apt install -y --no-install-recommends \
    bat \
    curl \
    fd-find \
    fzf \
    nano && \
apt clean && \
rm -rf /var/lib/apt/lists/*

cargo install \
    eza

cargo install zoxide --locked

## Install starship
curl -sS https://starship.rs/install.sh | sh -s -- -y

mkdir -p /tmp/setup-devcontainer-cli

cp ./* /tmp/setup-devcontainer-cli

chown -R ${CONTAINER_USER}:${CONTAINER_USER} /tmp/setup-devcontainer-cli
