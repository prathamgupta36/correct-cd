# ccd bash integration.   Add to ~/.bashrc:   eval "$(ccd init bash)"

__ccd_bin=ccd

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

# --- 2. typo / abbreviation-tolerant cd ----------------------------------
cd() {
  if [[ $# -ne 1 || $1 == -* ]]; then
    builtin cd "$@"
    return
  fi
  if builtin cd "$1" 2>/dev/null; then
    return 0
  fi
  local target
  target=$(command "$__ccd_bin" query "$1" --cwd "$PWD" 2>/dev/null)
  if [[ -n $target && -d $target ]]; then
    builtin cd "$target"
  else
    builtin cd "$1"
  fi
}
