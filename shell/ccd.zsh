# ccd zsh integration.   Add to ~/.zshrc:   eval "$(ccd init zsh)"

__ccd_bin=ccd

# Remove the old ccd-installed cd override when upgrading. Native cd and
# unrelated user cd functions are left alone.
if [[ "$(whence -w cd 2>/dev/null)" == "cd: function" ]] && whence -f cd 2>/dev/null | grep -q "__ccd_jump"; then
  unfunction cd
fi

# --- 1. log every directory change ---------------------------------------
autoload -Uz add-zsh-hook
__ccd_add() { command "$__ccd_bin" add "$PWD" &>/dev/null }
add-zsh-hook chpwd __ccd_add
__ccd_add   # record the current dir at shell startup

# enable dated history so future frecency is timestamped (past seed uses order)
setopt EXTENDED_HISTORY 2>/dev/null

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

# `ccd foo` jumps. Subcommands still go to the binary:
# `ccd stats`, `ccd query foo`, `ccd seed`, etc. Normal `cd` is untouched.
ccd() {
  if [[ $# -eq 0 || $# -gt 1 || $1 == -* ]] || __ccd_is_subcommand "$1"; then
    command "$__ccd_bin" "$@"
    return
  fi

  if [[ -d $1 ]]; then
    builtin cd "$1"
    return
  fi

  local target
  local -a matches
  target=$(command "$__ccd_bin" query "$1" --cwd "$PWD" 2>/dev/null)
  matches=("${(@f)$(command "$__ccd_bin" query "$1" --cwd "$PWD" --list --complete 2>/dev/null)}")
  if (( ${#matches} > 1 )); then
    print -u2 "ccd: options:"
    printf '  %s\n' "${matches[@]}" >&2
    return 1
  fi
  if [[ -n $target && ( ${#matches} -eq 0 || $target == $matches[1] ) ]]; then
    builtin cd "$target"
    return
  fi
  if (( ${#matches} )); then
    print -u2 "ccd: options:"
    printf '  %s\n' "${matches[@]}" >&2
    return 1
  fi

  if ! __ccd_jump "$1"; then
    print -u2 "ccd: no match: $1"
    return 1
  fi
}

# --- 3. ccd-only Tab UI --------------------------------------------------
if [[ -o interactive ]]; then
  typeset -ga __ccd_tab_matches
  typeset -g __ccd_tab_query=""
  typeset -g __ccd_tab_pwd=""
  typeset -gi __ccd_tab_index=0

  __ccd_tab_message() {
    emulate -L zsh
    local selected=$1
    local msg="ccd options:"
    local i marker
    for i in {1..${#__ccd_tab_matches}}; do
      marker=" "
      (( i == selected )) && marker=">"
      msg+=$'\n'" $marker $__ccd_tab_matches[$i]"
    done
    zle -M "$msg"
  }

  __ccd_tab() {
    emulate -L zsh

    if [[ $BUFFER != ccd\ * ]]; then
      __ccd_tab_query=""
      __ccd_tab_pwd=""
      __ccd_tab_index=0
      __ccd_tab_matches=()
      zle expand-or-complete
      return
    fi

    local current="${BUFFER#ccd }"
    local quoted_selected=""
    if (( __ccd_tab_index > 0 && __ccd_tab_index <= ${#__ccd_tab_matches} )); then
      quoted_selected="${(q)__ccd_tab_matches[$__ccd_tab_index]}"
    fi

    if [[ -z $__ccd_tab_query || $__ccd_tab_pwd != "$PWD" || ( -n $quoted_selected && "$current" != "$quoted_selected" ) && "$current" != "$__ccd_tab_query" ]]; then
      __ccd_tab_query="$current"
      __ccd_tab_pwd="$PWD"
      __ccd_tab_index=0
      __ccd_tab_matches=("${(@f)$(command "$__ccd_bin" query "$current" --cwd "$PWD" --list --complete 2>/dev/null)}")
      if (( ${#__ccd_tab_matches} )); then
        __ccd_tab_message 0
        return
      fi
      zle -M "ccd: no matches"
      return 1
    fi

    if (( ${#__ccd_tab_matches} == 0 )); then
      zle -M "ccd: no matches"
      return 1
    fi

    __ccd_tab_index=$(( (__ccd_tab_index % ${#__ccd_tab_matches}) + 1 ))
    BUFFER="ccd ${(q)__ccd_tab_matches[$__ccd_tab_index]}"
    CURSOR=$#BUFFER
    __ccd_tab_message $__ccd_tab_index
  }

  zle -N __ccd_tab
  bindkey '^I' __ccd_tab
fi
