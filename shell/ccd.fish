# ccd fish integration.   Add to config.fish:   ccd init fish | source

set -g __ccd_bin ccd

# Remove the old ccd-installed cd override when upgrading. Native cd and
# unrelated user cd functions are left alone.
if functions cd >/dev/null 2>&1; and functions cd | string match -q '*__ccd_jump*'
    functions -e cd
end

# --- 1. log every directory change ---------------------------------------
function __ccd_add --on-variable PWD
    command $__ccd_bin add "$PWD" >/dev/null 2>&1
end
command $__ccd_bin add "$PWD" >/dev/null 2>&1  # record current dir at startup

# --- 2. ccd jump command -------------------------------------------------
function __ccd_is_subcommand
    switch $argv[1]
        case add query seed init forget prune stats doctor help -h --help
            return 0
        case '*'
            return 1
    end
end

function __ccd_jump
    if test (count $argv) -ne 1; or string match -q -- '-*' $argv[1]
        return 2
    end
    if builtin cd $argv[1] 2>/dev/null
        return 0
    end
    set -l target (command $__ccd_bin query $argv[1] --cwd "$PWD" 2>/dev/null)
    if test -n "$target"; and test -d "$target"
        builtin cd "$target"
    else
        return 1
    end
end

function ccd
    if test (count $argv) -eq 0; or test (count $argv) -gt 1; or string match -q -- '-*' $argv[1]; or __ccd_is_subcommand $argv[1]
        command $__ccd_bin $argv
        return
    end

    if test -d $argv[1]
        builtin cd "$argv[1]"
        return
    end

    set -l target (command $__ccd_bin query $argv[1] --cwd "$PWD" 2>/dev/null)
    set -l matches (command $__ccd_bin query $argv[1] --cwd "$PWD" --list --complete 2>/dev/null)
    if test (count $matches) -gt 1
        printf 'ccd: options:\n' >&2
        for m in $matches
            printf '  %s\n' $m >&2
        end
        return 1
    end
    if test -n "$target"; and begin; test (count $matches) -eq 0; or test "$target" = "$matches[1]"; end
        builtin cd "$target"
        return
    end
    if test (count $matches) -gt 0
        printf 'ccd: options:\n' >&2
        for m in $matches
            printf '  %s\n' $m >&2
        end
        return 1
    end

    if not __ccd_jump $argv[1]
        printf 'ccd: no match: %s\n' $argv[1] >&2
        return 1
    end
end

function __ccd_complete
    command $__ccd_bin query (commandline -ct) --cwd "$PWD" --list --complete 2>/dev/null
end
complete -c ccd -f -a '(__ccd_complete)'
