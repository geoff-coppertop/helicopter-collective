#!/bin/sh

apt update && \
apt install -y --no-install-recommends \
    bat \
    btop \
    duf \
    fd-find \
    fish \
    fzf \
    jq \
    ripgrep \
    tree \
    unzip \
    vim \
    zip && \
apt clean && \
rm -rf /var/lib/apt/lists/*

## Debian installs these tools under different binary names
ln -sf /usr/bin/batcat /usr/local/bin/bat
ln -sf /usr/bin/fdfind /usr/local/bin/fd

cargo binstall -y \
    eza \
    du-dust \
    dua-cli \
    bottom \
    procs

cargo binstall -y zoxide
cargo binstall -y tealdeer
cargo binstall -y git-delta gitui

## Install starship
curl -sS https://starship.rs/install.sh | sh -s -- -y

## Install lazygit (Go binary, not available via cargo binstall)
LAZYGIT_VERSION=$(curl -sf https://api.github.com/repos/jesseduffield/lazygit/releases/latest | jq -r '.tag_name' | sed 's/^v//')
curl -sLo /tmp/lazygit.tar.gz "https://github.com/jesseduffield/lazygit/releases/download/v${LAZYGIT_VERSION}/lazygit_${LAZYGIT_VERSION}_Linux_x86_64.tar.gz"
tar -xzf /tmp/lazygit.tar.gz -C /usr/local/bin lazygit
rm /tmp/lazygit.tar.gz

## Install fastfetch (not in Debian bookworm standard repos)
FASTFETCH_VERSION=$(curl -sf https://api.github.com/repos/fastfetch-cli/fastfetch/releases/latest | jq -r '.tag_name')
curl -sLo /tmp/fastfetch.deb "https://github.com/fastfetch-cli/fastfetch/releases/download/${FASTFETCH_VERSION}/fastfetch-linux-amd64.deb"
dpkg -i /tmp/fastfetch.deb
rm /tmp/fastfetch.deb

## Set fish as default shell
usermod -s /usr/bin/fish $_CONTAINER_USER

mkdir -p /tmp/setup-devcontainer-cli

cp ./* /tmp/setup-devcontainer-cli

chown -R $_CONTAINER_USER:$_CONTAINER_USER /tmp/setup-devcontainer-cli
