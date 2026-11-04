#!/usr/bin/env sh
# Builds herdr-app-pluck and installs it into this checkout's mod:
#   1. cargo build --release
#   2. mod/bin/herdr-app-pluck  (the mod runs it from there)
#   3. herdr-app check mod
# With --link, also points $HERDR_APP_HOME/mods/pluck (default ~/.config/herdr-app/mods/pluck)
# at this checkout's mod/. An existing symlink there is replaced; anything else is left
# alone. Dotfiles that manage the link (mise) don't need --link.
set -eu

REPO_DIR=$(cd "$(dirname "$0")/.." && pwd)
APP_HOME="${HERDR_APP_HOME:-$HOME/.config/herdr-app}"
MOD_DIR="$REPO_DIR/mod"
MOD_LINK="$APP_HOME/mods/pluck"
LINK=0
for arg in "$@"; do
    case "$arg" in
        --link) LINK=1 ;;
        -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

log() { printf '%s\n' "$*" >&2; }

cd "$REPO_DIR"
cargo build --release
mkdir -p "$MOD_DIR/bin"
# Copy to a temp name then rename, so a running picker keeps its old binary intact.
cp target/release/herdr-app-pluck "$MOD_DIR/bin/.herdr-app-pluck.new"
mv -f "$MOD_DIR/bin/.herdr-app-pluck.new" "$MOD_DIR/bin/herdr-app-pluck"
log "Installed $MOD_DIR/bin/herdr-app-pluck"

if [ "$LINK" = 1 ]; then
    if [ -L "$MOD_LINK" ] && [ "$(readlink "$MOD_LINK")" = "$MOD_DIR" ]; then
        log "Already linked: $MOD_LINK"
    elif [ -L "$MOD_LINK" ]; then
        log "Replacing link $MOD_LINK -> $(readlink "$MOD_LINK")"
        rm "$MOD_LINK"
        ln -s "$MOD_DIR" "$MOD_LINK"
    elif [ -e "$MOD_LINK" ]; then
        log "Refusing to replace $MOD_LINK (not a symlink); move it aside and rerun"
        exit 1
    else
        mkdir -p "$(dirname "$MOD_LINK")"
        ln -s "$MOD_DIR" "$MOD_LINK"
        log "Linked $MOD_LINK -> $MOD_DIR"
    fi
fi

if command -v herdr-app >/dev/null 2>&1; then
    herdr-app check "$MOD_DIR"
fi
