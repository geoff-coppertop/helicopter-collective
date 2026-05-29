function fish_greeting
    fastfetch
end

function ls
    eza --icons=always $argv
end

function ll
    ls -lah $argv
end

function lt
    ll --tree --level=3 $argv
end

alias cat 'bat'
alias l 'ls -l'
alias .. 'cd ..'
alias ... 'cd ../..'

starship init fish | source
zoxide init fish | source

set -gx FZF_DEFAULT_COMMAND 'fd --type f'
set -gx FZF_CTRL_T_COMMAND 'fd --type f'
set -gx FZF_ALT_C_COMMAND 'fd --type d'

# fzf key bindings (Ctrl+R history, Ctrl+T file, Alt+C cd)
if test -f /usr/share/doc/fzf/examples/key-bindings.fish
    source /usr/share/doc/fzf/examples/key-bindings.fish
end
