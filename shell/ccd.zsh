# ccd zsh integration.   Add to ~/.zshrc:   eval "$(ccd init zsh)"

__ccd_bin=ccd

# --- 1. log every directory change ---------------------------------------
autoload -Uz add-zsh-hook
__ccd_add() { command "$__ccd_bin" add "$PWD" &>/dev/null }
add-zsh-hook chpwd __ccd_add
__ccd_add   # record the current dir at shell startup

# enable dated history so future frecency is timestamped (past seed uses order)
setopt EXTENDED_HISTORY 2>/dev/null

# --- 2. typo / abbreviation-tolerant cd ----------------------------------
# Tries the real builtin first, so native cd behavior is fully intact. Only on
# a "no such directory" miss does it consult ccd -> never hinders defaults.
cd() {
  # options or multi-arg forms (cd -, cd a b, cd -P x) pass straight through
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
    builtin cd "$1"   # re-run to surface the real error message
  fi
}

# --- 3. inline suggestion widget (v1) ------------------------------------
# Fills in ccd's best guess for a `cd <frag>` line so you can eyeball it
# and press Enter. Bound to a spare key (Ctrl-G), NOT Tab, so native Tab
# completion is untouched. The polished "Tab to preview, Tab again to
# complete" ghost-text UX is a separate, deferred design pass.
if [[ -o interactive ]]; then
  __ccd_suggest() {
    emulate -L zsh
    [[ $BUFFER == cd\ * ]] || return
    local frag=${BUFFER#cd }
    local target
    target=$(command "$__ccd_bin" query "$frag" --cwd "$PWD" 2>/dev/null)
    if [[ -n $target ]]; then
      BUFFER="cd $target"
      CURSOR=$#BUFFER
    fi
  }
  zle -N __ccd_suggest
  bindkey '^G' __ccd_suggest
fi
