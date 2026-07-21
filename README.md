# ccd

Correcting `cd`: typo- and abbreviation-tolerant directory jumping that learns
from your shell history.

```sh
cd Dwn        # ~/Downloads
cd Doanloads  # ~/Downloads
cd wn         # ~/dev/web-node
```

`ccd` never replaces normal `cd` behavior. The shell hook tries the real `cd`
first and only asks `ccd` for a match when the path does not exist.

## Install

```sh
cargo build --release
install -m755 target/release/ccd ~/.local/bin/ccd

ccd seed --dry-run --list
ccd seed

echo 'eval "$(ccd init zsh)"' >> ~/.zshrc
```

Use `ccd init bash` for Bash or `ccd init fish | source` for Fish.

## Commands

```sh
ccd add <path>
ccd query <fragment> [--cwd DIR] [--list]
ccd seed [--dry-run] [--list]
ccd init <zsh|bash|fish>
ccd stats
ccd prune [--dry-run]
ccd forget <path>
ccd doctor
```

## Data

The database is stored at:

```text
$XDG_DATA_HOME/ccd/db.tsv
```

or:

```text
~/.local/share/ccd/db.tsv
```

Override it with `CCD_DB`.

## License

MIT
