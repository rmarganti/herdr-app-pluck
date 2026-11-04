# herdr-app-pluck

Keyboard hints over the focused pane in the [Herdr](https://herdr.dev) desktop app
(`herdr-app`). Press a key and every URL, path, git SHA, UUID, IP, number (and your own
patterns) on screen gets a one- or two-letter hint, tmux-fingers style. Type the hint to
copy the token, or to open the URL.

Nothing on screen moves: pluck replays exactly what the pane shows, dims it, and prints
hints over the first letters of each token.

This is the app-only rebuild of [herdr-pluck](../herdr-pluck), which stays the plugin
for the Herdr TUI. Both read the same config file.

## Install

Needs Rust (1.96+), Zig (for libghostty-vt, fetched at build time) and herdr-app 0.1.12+
with herdr-core 0.1.17+. macOS (Apple Silicon) is tested; Linux should work.

```sh
./scripts/install.sh          # build, put the binary in mod/bin/, herdr-app check mod
./scripts/install.sh --link   # also link ~/.config/herdr-app/mods/pluck -> mod/
```

`--link` replaces an existing *symlink* at `mods/pluck` (for example the old
herdr-pluck mod) and refuses to touch a real folder. Skip it if your dotfiles own the link.

Then, in `~/.config/herdr-app`:

1. **Keys** (`keys.json`), any free keys:

   ```json
   { "global": { "cmd-shift-f": "pluck.copy", "cmd-shift-u": "pluck.open" } }
   ```

   `pluck.open-url` still works as an alias for `pluck.open`.

2. **A place in your layout.** Key commands only reach mods that are in the shown
   layout, and pluck draws nothing. Either a floating block (no divider, nothing shown),
   added to your layout root's `floating` list:

   ```json
   { "id": "pluck", "content": { "mod": "pluck" }, "edge": "bottom", "align": "end" }
   ```

   or an empty block as the last child of the root (draws a 1px divider if the root
   has `divider`):

   ```json
   { "id": "pluck", "type": "block", "content": { "mod": "pluck" }, "size": { "px": 0 } }
   ```

3. If `mods/pluck` already existed (the old herdr-pluck mod), the running copy may keep
   the old code: switch layouts away and back once (`ctrl-shift-l`, or
   `herdr-app do layout.switch`).

## Use

| Key | `pluck.copy` | `pluck.open` |
|---|---|---|
| hint letter(s) | copy | open |
| Shift on the final letter | open if it's a URL, else copy | copy |
| Backspace | remove the last typed letter | same |
| Esc, Ctrl-C | cancel | cancel |

- `pluck.copy` hints every token type; `pluck.open` hints only `http://`, `https://` and
  `file://` URLs.
- 26 or fewer distinct values get one letter, more get two (up to 676). The same text
  gets the same hint everywhere it appears.
- Tokens that soft-wrap across rows are one token.
- OSC 8 hyperlinks (named links) are hinted on their visible text and pick their hidden
  target. *herdr-core 0.1.17's repaint doesn't carry OSC 8, so named links aren't hinted
  yet; they will be once it does.*
- Copies go straight to the system clipboard; a short toast confirms them.
- Only local terminals for now; on a remote space pluck says so.

### Known limitations

- **Scrolled-up panes** (below).
- The **dimmed** view drops background colors and reverse video (highlighted lines,
  status bars show plain). Blending them instead is planned.
- A program's **cursor** is hidden while pluck is open.

### Scrolled-up panes

The app scrolls each pane on its own and doesn't tell anyone where. If you've scrolled a
pane up, pluck shows (and hints) the program's live screen, so the view jumps to the
bottom. Output that arrives while pluck is open doesn't move anything: it shows a frozen
snapshot.

## Config

Shared with herdr-pluck: `$XDG_CONFIG_HOME/herdr-pluck/config.toml`, else
`~/.config/herdr-pluck/config.toml`. It's read on every invocation.

```toml
[[patterns]]
name = "ticket"
regex = "\\bABC-[0-9]+\\b"
priority = 25            # lower wins overlaps; built-ins are 10 (url) to 60 (number)

[[patterns]]
name = "trace"
regex = "trace_id=(?<match>[a-z0-9]+)"   # a `match` group picks only that part

[project]
patterns = true                       # read project files (default true)
pattern_files = [".herdr-pluck.toml"] # searched from the pane's folder up to the git root
```

- Project files have the same shape; only their `[[patterns]]` count, and they come
  before the global ones.
- `clipboard` is herdr-pluck's key; pluck accepts it and ignores it.
- Any other unknown key makes the whole file invalid: pluck says so in a toast
  (`config.toml: unknown field `foo` ...`) and uses no custom patterns. A bad regex skips
  just that pattern, also with a toast.
- Custom patterns are for copying; `pluck.open` uses only the built-in URL detector.

## How it works

1. The mod gets `pluck.copy`, checks the focused pane, and starts
   `herdr-app-pluck pick` in its own herdr-core terminal (`hc run --bg --rm`).
2. The picker captures the pane's terminal with `hc attach --observe` (a full repaint),
   decodes it with libghostty-vt (the same emulator herdr-core uses, so widths agree),
   finds tokens and writes `ready`.
3. The mod shows the picker in the pane (`terminal.show`) and re-sends until the app has
   given it the pane's size.
4. The picker replays the capture byte for byte, minus anything that changes input
   (mouse, kitty keyboard, ...) and with colors rewritten to dim, draws hints, reads keys
   and writes `result.json`.
5. The picker exits, the pane gets its terminal back, and the mod copies (native
   clipboard) or opens (`open_url`; `file://` through the system opener).

## Development

See `AGENTS.md` and the verify skill in `.agents/skills/verify-herdr-app-pluck/`.

```sh
cargo fmt --all -- --check && cargo clippy --all-targets && cargo test
```
