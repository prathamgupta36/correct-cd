#!/usr/bin/env zsh
# End-to-end integration test: seed from history, enable the shell hook,
# then drive real `cd` commands and assert we land in the right place.

emulate -L zsh
fail=0
pass() { print -P "%F{green}  PASS%f  $1" }
bad()  { print -P "%F{red}  FAIL%f  $1"; fail=1 }
check() { # desc  expected  actual
  if [[ "$2" == "$3" ]]; then pass "$1"; else bad "$1 (expected '$2', got '$3')"; fi
}

: "${HOME:=/home/tester}"
export HOME
: "${CCD_DB:=$HOME/.local/share/ccd/db.tsv}"
export CCD_DB
builtin cd $HOME

# --- fake directory tree + shell history --------------------------------
mkdir -p Downloads Documents Desktop dev/web-node dev/web-node/src \
         dev/ccd/src projects/photography
cat > .zsh_history <<'EOF'
cd ~/Downloads
cd ~/Downloads
cd ~/Downloads
cd ~/Documents
cd ~/dev/web-node
cd ~/dev/web-node
cd src
cd ~/projects/photography
EOF

print "== 1. seed from history =="
ccd seed
print "   DB now has $(wc -l < "$CCD_DB" | tr -d ' ') dirs"

print "\n== 2. enable shell integration (non-interactive) =="
eval "$(ccd init zsh)"
whence -w cd | grep -q builtin && pass "native cd remains builtin" || bad "native cd was modified"
whence -w ccd | grep -q function && pass "ccd jump function installed" || bad "ccd jump function missing"

print "\n== 3. behavior tests =="

builtin cd $HOME; ccd Dwn 2>/dev/null
check "ccd Dwn (explicit jump) -> Downloads" "$HOME/Downloads" "$PWD"

builtin cd $HOME; ccd "$HOME/Documents" 2>/dev/null
check "ccd selected absolute path -> Documents" "$HOME/Documents" "$PWD"

builtin cd $HOME; ccd Doanloads 2>/dev/null
check "ccd Doanloads (typo) -> Downloads" "$HOME/Downloads" "$PWD"

builtin cd $HOME; ccd documnets 2>/dev/null
check "ccd documnets (typo) -> Documents" "$HOME/Documents" "$PWD"

q=$(ccd query Dwn --cwd $HOME)
check "ccd query still reaches binary" "$HOME/Downloads" "$q"

builtin cd $HOME; cd Dwn 2>/dev/null
check "cd Dwn stays native and does not jump" "$HOME" "$PWD"

builtin cd $HOME; cd Doanloads 2>/dev/null
check "cd Doanloads stays native and does not jump" "$HOME" "$PWD"

builtin cd $HOME; cd documnets 2>/dev/null
check "cd documnets stays native and does not jump" "$HOME" "$PWD"

# native behavior must be untouched: real paths work exactly as before
builtin cd $HOME; cd /tmp
check "cd /tmp (real path, native) works" "/tmp" "$PWD"

builtin cd $HOME; cd Desktop
check "cd Desktop (real relative dir) native, no fuzzing" "$HOME/Desktop" "$PWD"

# genuine garbage: fail and stay put, like normal cd
builtin cd $HOME; cd zzznope 2>/dev/null
check "cd zzznope (no match) stays put" "$HOME" "$PWD"

# the chpwd hook logs visits: after visiting, a query finds the child
builtin cd $HOME/dev/ccd/src
q=$(ccd query src --cwd $HOME/dev/ccd)
check "chpwd hook logged visit; child 'src' found" "$HOME/dev/ccd/src" "$q"

print ""
if [[ $fail -eq 0 ]]; then
  print -P "%F{green}=== ALL INTEGRATION TESTS PASSED ===%f"
else
  print -P "%F{red}=== SOME TESTS FAILED ===%f"; exit 1
fi
