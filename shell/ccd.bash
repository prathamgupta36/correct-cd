# ccd bash integration.   Add to ~/.bashrc:   eval "$(ccd init bash)"

__ccd_bin=ccd

# Remove the old ccd-installed cd override when upgrading. Native cd and
# unrelated user cd functions are left alone.
if declare -f cd >/dev/null 2>&1 && declare -f cd | grep -q "__ccd_jump"; then
  unset -f cd
fi

# --- 1. log a directory change (only when PWD actually changed) -----------
__ccd_add() {
  if [[ $PWD != "${__ccd_last:-}" ]]; then
    __ccd_last=$PWD
    command "$__ccd_bin" add "$PWD" >/dev/null 2>&1
  fi
}
if declare -p PROMPT_COMMAND 2>/dev/null | grep -q 'declare \-a'; then
  case " ${PROMPT_COMMAND[*]} " in
    *" __ccd_add "*) ;;
    *) PROMPT_COMMAND=(__ccd_add "${PROMPT_COMMAND[@]}") ;;
  esac
else
  case ";${PROMPT_COMMAND:-};" in
    *";__ccd_add;"*) ;;
    *) PROMPT_COMMAND="__ccd_add;${PROMPT_COMMAND:-}" ;;
  esac
fi

# --- 2. ccd jump command -------------------------------------------------
__ccd_is_subcommand() {
  case "$1" in
    add|query|seed|init|forget|prune|stats|doctor|help|-h|--help) return 0 ;;
    *) return 1 ;;
  esac
}

__ccd_jump() {
  [[ $# -eq 1 && $1 != -* ]] || return 2
  if builtin cd "$1" 2>/dev/null; then
    return 0
  fi
  local target
  target=$(command "$__ccd_bin" query "$1" --cwd "$PWD" 2>/dev/null)
  if [[ -n $target && -d $target ]]; then
    builtin cd "$target"
  else
    return 1
  fi
}

ccd() {
  if [[ $# -eq 0 || $# -gt 1 || $1 == -* ]] || __ccd_is_subcommand "$1"; then
    command "$__ccd_bin" "$@"
    return
  fi

  if [[ -d $1 ]]; then
    builtin cd "$1"
    return
  fi

  local target matches first
  target=$(command "$__ccd_bin" query "$1" --cwd "$PWD" 2>/dev/null)
  matches=$(command "$__ccd_bin" query "$1" --cwd "$PWD" --list --complete 2>/dev/null)
  first=${matches%%$'\n'*}
  if [[ $(grep -c . <<< "$matches") -gt 1 ]]; then
    printf 'ccd: options:\n' >&2
    while IFS= read -r line; do
      printf '  %s\n' "$line" >&2
    done <<< "$matches"
    return 1
  fi
  if [[ -n $target && ( -z $matches || $target == "$first" ) ]]; then
    builtin cd "$target"
    return
  fi
  if [[ -n $matches ]]; then
    printf 'ccd: options:\n' >&2
    while IFS= read -r line; do
      printf '  %s\n' "$line" >&2
    done <<< "$matches"
    return 1
  fi

  if ! __ccd_jump "$1"; then
    printf 'ccd: no match: %s\n' "$1" >&2
    return 1
  fi
}

__ccd_complete() {
  local cur="${COMP_WORDS[COMP_CWORD]}"
  local matches
  matches=$(command "$__ccd_bin" query "$cur" --cwd "$PWD" --list --complete 2>/dev/null)
  if [[ -n $matches ]]; then
    COMPREPLY=()
    local line
    while IFS= read -r line; do
      COMPREPLY+=("$line")
    done <<< "$matches"
  else
    COMPREPLY=($(compgen -W "add query seed init forget prune stats doctor help" -- "$cur"))
  fi
}
complete -o filenames -F __ccd_complete ccd
