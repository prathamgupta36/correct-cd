# ccd fish integration.   Add to config.fish:   ccd init fish | source

set -g __ccd_bin ccd

# --- 1. log every directory change ---------------------------------------
function __ccd_add --on-variable PWD
    command $__ccd_bin add "$PWD" >/dev/null 2>&1
end
command $__ccd_bin add "$PWD" >/dev/null 2>&1  # record current dir at startup

# --- 2. typo / abbreviation-tolerant cd ----------------------------------
function cd
    if test (count $argv) -ne 1; or string match -q -- '-*' $argv[1]
        builtin cd $argv
        return
    end
    if builtin cd $argv[1] 2>/dev/null
        return 0
    end
    set -l target (command $__ccd_bin query $argv[1] --cwd "$PWD" 2>/dev/null)
    if test -n "$target"; and test -d "$target"
        builtin cd "$target"
    else
        builtin cd $argv[1]
    end
end
