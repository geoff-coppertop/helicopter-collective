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

## Configure starship
mkdir -p ~/.config
cp ./starship.toml ~/.config/starship.toml

## Configure bash
mkdir -p ~/.bashrc.d
cp ./aliases ~/.bashrc.d/aliases
cp ./bashrc ~/.bashrc

## Configure git
mkdir -p ~/.config/git
cp ./gitconfig ~/.gitconfig
cp ./commit.template ~/.config/git/commit.template

cd -

rm -rf ${DIR}
