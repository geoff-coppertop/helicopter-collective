#!/bin/sh

DIR=$(dirname "$(readlink -f "$0")")
if [ -z "$DIR" ]; then
  echo "Error: Unable to determine the directory of the script."
  exit 1
fi
if [ ! -d "$DIR" ]; then
  echo "Error: Directory $DIR does not exist."
  exit 1
fi

cd ${DIR}

echo $PWD

## Configure fish
mkdir -p ~/.config/fish
cp ./config.fish ~/.config/fish/config.fish

## Configure bash (fallback)
mkdir -p ~/.bashrc.d
cp ./aliases ~/.bashrc.d/aliases
cp ./bashrc ~/.bashrc

## Configure git
## Written to the XDG path so VS Code's auto-forwarded ~/.gitconfig (user identity) takes precedence
mkdir -p ~/.config/git
cp ./gitconfig ~/.config/git/config
cp ./commit.template ~/.config/git/commit.template

cd -

rm -rf ${DIR}
