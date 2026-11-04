# herdr-app-pluck — plan

Keyboard hints over the focused herdr-app pane: type a hint to copy a visible
token or open a visible URL. A rebuild of
[herdr-pluck](../herdr-pluck) (Herdr TUI plugin) for the Herdr desktop app
(`herdr-app`) only, borrowing ideas from
[diegoeche/herdr-link-hints](https://github.com/diegoeche/herdr-link-hints).

This file is the full hand-off. The implementer starts in a fresh session with
no other context: read this file end to end before touching code.

---

## 0. Ground rules

- **Hard requirement: placement is exact.** When pluck opens, no character on
  screen may move. Hints and colors change; glyph positions never do. A
  build that shifts text is broken, not "mostly working". The only accepted
  exception is in [§4.6](#46-known-limitation-scrolled-up-panes).
- **Tracer bullet first.** Build the thinnest end-to-end slice, prove
  exactness, then widen. Order is in [§8](#8-build-order).
- **When this plan is silent, copy herdr-pluck's behavior.** It is the
  reference for patterns, priorities, hint widths, config semantics and
  key handling. Path: `~/code/rmarganti/herdr-pluck`.
- **Ask the user in plain chat text, never with an ask-user/question tool.**
- **Verify every feature yourself, in the real app, before claiming it
  done** ([§7.1](#71-verifying-features-as-an-agent)). Unit tests alone don't
  count. Read `~/.config/herdr-app/AGENTS.md`, `herdr-app --skill` and
  `hc --skill` at the start of the session.

## 1. Environment facts (checked 2026-11-03)

| Thing | Value |
|---|---|
| herdr-app | 0.1.12, `~/.local/bin/herdr-app` → `~/Applications/Herdr.app` |
| herdr-core (`hc`) | 0.1.17, `~/.local/bin/hc` → `herdr-core` |
| herdr-core's emulator | **libghostty-vt** (found in the binary's strings) |
| App home | `~/.config/herdr-app` (or `$HERDR_APP_HOME`) |
| App docs for mods | `~/.config/herdr-app/AGENTS.md` (read it), `herdr.d.ts`, `gpui-kit.d.ts` |
| Dotfiles | `~/.dotfiles`; `mise.toml` symlinks config, e.g. `"~/.config/herdr-app/mods/smart-tabs" = {}` from `dots/.config/herdr-app/...` |
| Old pluck in app today | `~/.config/herdr-app/mods/pluck` → `~/code/rmarganti/herdr-pluck/herdr-app/mods/pluck` (symlink, **not** managed by mise) |
| Platform | macOS (Apple Silicon). Linux support is nice-to-have, not required |

Gotchas already learned:

- **Mod hot reload can go stale.** After editing a mod, if behavior doesn't
  change, switch the layout away and back (`herdr-app do layout.switch`).
- `herdr-app check <mod folder>` validates a mod; `herdr-app status` shows
  errors; `herdr-app shot out.png` saves the exact last window frame (physical
  pixels, works while covered). These are the main debugging tools.
- Mod `exec()` has a **30 s limit and 8 MiB output cap.** Never block a mod
  call on the user's thinking time.
- No `setTimeout` in mods: use `cx.timer.every(ms, cb)` / `cx.spawn`.
- Key commands `"<mod>.<command>"` only reach a mod that is **in the shown
  layout** (the old pluck README documents this; re-verify). The mod draws
  nothing, so it lives in a `size: {px: 0}` block.

## 2. Decisions (settled with the user)

| # | Topic | Decision |
|---|---|---|
| 1 | Scope | **New, app-only project.** No TUI plugin support. herdr-pluck stays frozen for TUI users |
| 2 | Repo | **Separate repo** at `~/code/rmarganti/herdr-app-pluck`. Dotfiles own only the wiring (symlink + keys + layout block) |
| 3 | Placement | **Terminal swap.** A picker program runs in its own herdr-core terminal and is shown in the source pane with `terminal.show`. No pixel math, no overlays, no list fallback |
| 4 | Exactness mechanism | Picker **replays the raw `hc attach --observe` capture verbatim** so herdr's own emulator lays out text. Our decoder only decides where hints go |
| 5 | Scrollback | Pluck targets the **live screen**. If the app pane is scrolled up, the view jumps to the live screen: documented, accepted |
| 6 | Split of work | **JS mod**: key → launch picker → show it → wait for result → native copy/open → toasts. **Rust picker**: capture → match → draw → read keys → write result |
| 7 | Actions | `pluck.copy` (all token types; hint copies; Shift+hint opens if URL, else copies) and `pluck.open` (URLs only; hint opens; Shift+hint copies) |
| 8 | Open targets | URLs only (`http://`, `https://`, `file://`). No opening paths in editors/Finder |
| 9 | OSC 8 links | *(Provisional — user will judge in manual testing.)* Hint on the visible text; value is the hidden URL; an OSC 8 span beats every pattern |
| 10 | Machines | **Local terminals only** for the tracer bullet. Remote is expected later (see [§9](#9-later-not-in-the-tracer-bullet)) |
| 11 | Clipboard | Native `cx.write_to_clipboard` in the mod. No OSC 52, no `pbcopy`, no `clipboard` setting |
| 12 | Config | **TOML**, shared file `~/.config/herdr-pluck/config.toml` (same as old app host). Project files `.herdr-pluck.toml`, same name and schema |
| 13 | Look | **Hint over the first cells of the token** (tmux-fingers style, as today), rest of token highlighted, **everything else dimmed** |
| 14 | Dimming | Done by **rewriting SGR parameters only** in the replayed bytes (never by redrawing cells). Tracer bullet: all text fg → theme `dim` color, backgrounds stripped, reverse video stripped. Later: blend backgrounds instead of stripping |
| 15 | Language | Rust picker, starting from herdr-pluck's crate. JS for the mod |

## 3. Architecture

```text
keys.json  cmd-shift-f → pluck.copy     cmd-shift-u → pluck.open   (any free keys)

mod "pluck" (JS, 0px layout block)                    picker (Rust, own hc terminal)
──────────────────────────────────                    ───────────────────────────────
nextCommand() → "copy" | "open"
f = focused(); need f.pane && (f.shown ?? f.terminal)
reject if terminal address contains "/" (remote)
mkdir run dir  $TMPDIR/herdr-app-pluck/<nonce>/
hc run --bg --json --rm --no-parent --no-notify \
   --cwd <live cwd> -- <modDir>/bin/herdr-app-pluck pick \
   --source <t> --action copy|open --run-dir <dir> \
   --config ~/.config/herdr-pluck/config.toml \
   --dim-color <theme().dim> --hint-colors ...
                                         ───────────▶ hc get --json <source>     (cols, rows, cwd)
                                                      hc attach --observe --history 0 <source>
                                                         capture until quiet (see §4.2)
                                                      decode → cell grid (+ OSC 8 per cell)
                                                      load config, match, assign hints
                                                      no matches → write {"outcome":"no_matches"}, exit
                                                      write <run-dir>/ready
wait for <run-dir>/ready (watchFolder + timer)  ◀───
run("terminal.show", {terminal: picker, pane})
re-send until `hc get picker` cols/rows == source  (old herdr-pluck: ≤3 s, retry 250 ms)
                                                      wait own size == source size (≤5 s)
                                                      write sanitized + dimmed capture verbatim
                                                      draw hints + highlights on top
                                                      read keys (raw mode)
                                                      write <run-dir>/result.json, exit
                                                      (--rm: terminal removed; pane shows its own again)
watchFolder(run dir) / cx.timer poll → read result.json
copy  → cx.write_to_clipboard(text) (+ small toast)
open  → cx.open_url(url)
error / timeout / picker gone without result → notify(level:error)
rm run dir
```

Why each piece is where it is:

- Only the mod can call `cx.write_to_clipboard` / `cx.open_url` / `notify`.
- Only a process in a terminal can draw an exact grid over the pane.
- The mod must not `await exec("hc", ["run", ...])` without `--bg`: `exec`
  is capped at 30 s and the user may take longer. Launch with `--bg`, then
  wait on files.
- **Capture before show.** The picker captures first and signals `ready`;
  only then does the mod swap the pane. This guarantees the snapshot is what
  the user saw and avoids any chance the app resizes the hidden source.

## 4. The picker in detail

### 4.1 CLI

```text
herdr-app-pluck pick --source <terminal> --action <copy|open> --run-dir <dir>
                     [--config <path>] [--dim-color #rrggbb]
                     [--hint-fg #rrggbb --hint-bg #rrggbb --match-fg #rrggbb]
                     [--no-dim]          # debug: exactness harness
                     [--capture-out <f>] # debug: save raw capture bytes
```

Colors come from the mod's `theme()` (`dim`, `accent`, `accent_foreground`,
`foreground`, `background`) so pluck follows the user's theme. Pick sensible
defaults when absent.

`result.json` shapes:

```json
{"outcome":"picked","action":"copy","text":"src/app/mod.rs","is_url":false}
{"outcome":"picked","action":"open","text":"https://example.com"}
{"outcome":"cancelled"}
{"outcome":"no_matches"}
{"outcome":"error","message":"..."}
```

`action` is the *final* action after Shift is applied (§4.7). The mod never
has to reason about Shift.

### 4.2 Capture

Port link-hints' `snapshot.py` behavior to Rust:

- `hc get --json <source>` → `cols`, `rows`, `foreground.cwd ?? cwd`
  (`TerminalFacts::live_cwd` in herdr-pluck `src/app/hc.rs` already does this;
  copy it).
- Spawn `hc attach --observe --history 0 <source>` with stdin **piped and
  never written** (closing it may end the session), stdout piped, stderr null.
  `--observe` cannot type or resize.
- Read until: 5 s with no first byte → error; after the first byte, stop on a
  120 ms quiet gap, or 400 ms after first byte for constantly-updating
  programs; cap 4 MiB → error.
- Kill only our observer child; wait for it.
- **Spike item:** confirm what the observer emits first (does it start with a
  full repaint including mode sets like `?1049h`, cursor position, SGR?). Save
  real captures with `--capture-out` into `fixtures/captures/` for tests.

### 4.3 Decode (cell grid)

Needed for: matching text by row/col, cell widths, OSC 8 targets per cell.

The core risk is our decoder disagreeing with herdr's emulator on cell widths
(emoji, ZWJ, CJK, combining marks, VS16). Herdr-core uses **libghostty-vt**.
In order of preference:

1. A Rust binding to libghostty-vt, if one exists and builds cleanly on macOS.
   Same emulator means same widths.
2. `alacritty_terminal` (mature grid, wide chars, OSC 8 hyperlinks per cell).
3. `vt100` crate + a hand-written OSC 8 tracker.
4. Port link-hints' `terminal_grid.py` (bounded decoder) — last resort.

Spend at most a short spike choosing; the exactness harness (§7) is the
judge. Whatever the decoder, anything it flags as unsafe (unsupported
sequences, scroll regions it can't model) means: **do not draw hints on cells
it's unsure of**, never guess.

Text for matching: build each visible row's string from the grid, keeping a
byte-offset → column map, so pattern matches (byte ranges) become cell
ranges. Soft-wrapped lines: the old pluck matched per logical line; for the
tracer bullet match per screen row, then (step 6) join rows the emulator
marks as soft-wrapped so long URLs/paths match across the wrap.

### 4.4 Replay (exactness core)

1. Wait until our terminal size == source `cols × rows` (copy
   `wait_for_terminal_size` from herdr-pluck `src/picker/session.rs`).
2. Enter raw mode, hide cursor.
3. **Sanitize** the capture: drop sequences that change *input* behavior of
   our terminal, keep everything that affects *display*:
   - drop: mouse modes `?1000 ?1002 ?1003 ?1005 ?1006 ?1015 ?1016`, bracketed
     paste `?2004`, focus events `?1004`, kitty keyboard `CSI > … u` /
     `CSI = … u` / `CSI < u`, `modifyOtherKeys` `CSI > 4 ; … m`, application
     cursor keys `?1` (so arrows/Esc arrive normally), DSR/DA queries that
     would make the emulator reply into our stdin.
   - keep: alternate screen `?1049`, autowrap `?7`, origin mode, scroll
     regions, all printable bytes, all cursor movement, OSC 8.
4. **Dim** (unless `--no-dim`): rewrite every SGR `CSI … m` *in place*:
   - remove fg params (`30–37`, `39`, `90–97`, `38;5;n`, `38;2;r;g;b`, and the
     colon forms `38:5:n`, `38:2::r:g:b`), remove bg params (`40–47`, `49`,
     `100–107`, `48;…`), remove reverse `7`;
   - append `38;2;<dim>` and `49`; a bare reset (`CSI m` / `CSI 0 m`) becomes
     `CSI 0;38;2;<dim>;49 m`;
   - also emit `CSI 38;2;<dim> m` once before the capture so text before the
     first SGR is dimmed.
   - Keep bold/italic/underline/strike.
   SGR never moves the cursor or changes widths, which is why this preserves
   exactness. Unit-test the rewriter heavily (pure function, bytes → bytes).
5. Write the bytes verbatim to stdout. Do **not** re-encode text.
6. Draw hints (§4.5). Restore cursor visibility only on exit.

### 4.5 Hints and highlights

- Pattern matching, priorities, overlap resolution, duplicate handling and hint
  assignment: port herdr-pluck `src/patterns.rs` and `src/hints.rs` as is.
  Alphabet `asdfghjklqwertyuiopzxcvbnm`; 1 letter if ≤26 unique texts, else 2
  letters for all; max 26² unique; same text → same hint at every occurrence;
  assignment order top-to-bottom, left-to-right.
- `pluck.open` uses only URL matches (old `find_openable_urls`) plus OSC 8
  links whose target is `http(s)://` or `file://`. Old pluck ignored custom
  patterns for open-url: keep that.
- OSC 8: a contiguous run of cells with the same hyperlink target is one
  match, value = target, priority above `url` (use 5). It suppresses any
  pattern match overlapping it.
- Drawing per occurrence: `CUP` to the token's first cell, hint style, print
  the hint letters over the token's first 1–2 cells; then match style and
  re-print the rest of the token's characters.
  - **Only redraw a cell when it is a single-width, non-combining character**
    per the decoder. If any cell of the token isn't, draw only the hint letters
    over the first cells if *those* are single-width; otherwise skip that
    occurrence's highlight entirely (the hint still exists at other
    occurrences, or skip it if none are safe). Never print a wide char
    ourselves.
- While typing (2-letter hints): after the first letter, **replay the full
  sanitized/dimmed capture again**, then draw only hints that still match the
  prefix (typed letter shown in a distinct style). Backspace removes the
  letter the same way. Restoring cells always comes from the original bytes,
  never from our decoder.

### 4.6 Known limitation: scrolled-up panes

The app scrolls each terminal independently and exposes no scroll offset (no
`terminal.scroll`, nothing in `panes()`/`terminals()`/`hc`). If the pane is
scrolled up, the picker shows the program's live screen and the view appears
to jump. Accepted; document it in the README. Live output that arrives while
the picker is open is not a jump: the picker shows a frozen snapshot.

### 4.7 Keys

| Key | `pluck.copy` | `pluck.open` |
|---|---|---|
| hint letter(s) | copy | open |
| Shift on the final letter | open if URL, else copy | copy |
| Backspace | remove last typed letter | same |
| Esc, Ctrl-C | cancel | cancel |
| letter not starting any hint | ignore | ignore |
| no matches | (never shown: mod toasts "No matches") | same |

If the source program had kitty keyboard / application modes on, §4.4 step 3
stops them leaking into the picker; verify Shift+letter arrives as uppercase.

## 5. The mod

Folder `mod/` in the repo, symlinked as `~/.config/herdr-app/mods/pluck`.

- `mod.json`: `{ "place": "none", "title": "Pluck" }` (old one; verify it still
  works). `main.js`, `jsconfig.json`, `.gitignore` with `gpui-kit.d.ts`
  (auto-generated), `bin/` (gitignored, holds the built binary).
- Locate the binary: `appHome() + "/mods/pluck/bin/herdr-app-pluck"`.
  Check it exists first; if not, toast "Run scripts/install.sh".
- Command loop: copy the old mod's pattern (`herdr-pluck/herdr-app/mods/pluck/main.js`):
  `cx.spawn(async (cx) => { for (;;) { const c = await nextCommand(); …; cx.notify(); } })`.
  Calling `cx.notify()` lets hot reload end the old loop.
- Target: `const f = focused(); const term = f?.shown ?? f?.terminal;` require
  `f.pane` and `term`. A layout terminal block has no pane → toast
  "Focus a terminal pane first." A `/` in `term` → toast "Pluck only works on
  local terminals for now."
- Ignore a second invocation while one is running (keep a `busy` flag).
- Launch, wait for `ready` (or a result like `no_matches`/`error`), show,
  retry show until sized, wait for `result.json`, act, clean up — as in §3.
  Waiting: `watchFolder(runDir)` plus a `cx.timer` fallback; also give up if
  `hc get <picker>` says not found and no result exists (picker crashed) and
  toast an error.
- Use `run("terminal.show", { terminal, pane })` from the mod (not
  `herdr-app do`).
- Toasts via `notify({ title: "Pluck", body, level })`. Copy toast is short
  ("Copied src/app/mod.rs", truncated).
- Pass theme colors from `theme()` to the picker.
- Label the picker terminal (`hc run … --label pluck=picker`) so
  verification can find it.
- Test-only hooks from `<appHome>/pluck-dev.json` (`dry_run_open`,
  `keep_run_dirs`), inert when the file is absent (§7.1).

## 6. Config

Port herdr-pluck `src/config.rs` with these changes:

- Global path: `$XDG_CONFIG_HOME/herdr-pluck/config.toml`, else
  `~/.config/herdr-pluck/config.toml` (passed by the mod or resolved by the
  binary — same result).
- **Shared with the old plugin.** Accept and ignore `clipboard` (don't
  reject). Keep `deny_unknown_fields` for everything else.
- **Divergence rule:** the first time the new tool needs a key the old plugin
  doesn't know, stop and ask the user: loosen the old plugin's parsing or
  split the files. Don't add new keys silently (the old plugin would reject
  the whole file and lose the user's patterns).
- Project files: `.herdr-pluck.toml`, searched from the source's live cwd up
  to the git root, first found wins, `[[patterns]]` only; `[project]` table in
  global config controls it. Same semantics as the README of herdr-pluck.
- Config errors: picker writes them into `result.json` / a `warnings` field
  and the mod toasts them ("config.toml: unknown key `foo`, using defaults").
  Old behavior was silent logging.
- Read on every invocation; no caching.

## 7. Validation

Two layers, both mandatory:

- **7.1 Agent-driven feature verification**: every user-facing feature is
  proven by driving the *real* app end to end, the way the user would, and
  capturing evidence. This is how the implementing agent checks its own
  work at every step.
- **7.2–7.6 Exactness harness and tests**: automated checks that no
  character moves.

### 7.1 Verifying features as an agent

`cargo test` passing is never proof a feature works. Each feature is done
only when an agent has driven it through the real app and saved evidence.

**Read first, every new session:**

1. `~/.config/herdr-app/AGENTS.md`: the installed guide (commands, keys,
   mods, `status`/`shot`/`check`). It is regenerated by the app; never edit.
2. `herdr-app --skill`: how an agent drives the person's window safely
   (inspect before acting, use IDs from `ls`, never restart the app/core).
3. `hc --skill`: how to start, read, type into and clean up terminals.
4. Prior art: `~/code/rmarganti/herdr-pluck/.agents/skills/verify-herdr-pluck/`
   — `SKILL.md` (structure, evidence, proof standards) and
   `features/herdr-app.md` (already-worked-out recipe for driving pluck in
   herdr-app, plus gotchas). Reuse its `fixtures/` (`tokens.txt`, `many.txt`,
   `no-urls.txt`, `custom-patterns.txt`, configs).

**Toolbox (all verified to exist in herdr-app 0.1.12 / hc 0.1.17):**

| Need | Command |
|---|---|
| See windows, spaces, tabs, pane & terminal IDs | `herdr-app ls` |
| Layout, focus, overlays, zoom, **mod/file errors** | `herdr-app status` |
| Load the mod once, exact error on failure | `herdr-app check <mod dir>` |
| Pixel-exact frame of the window | `herdr-app shot <file.png>` (then *look at it* with the image reader) |
| Make a fixture tab | `herdr-app do tab.new --space <id> --name pluck-test --argv '["<fixture script>"]'` |
| Splits / zoom / app zoom | `herdr-app do pane.split --direction right --pane <p>`, `pane.zoom`, `app.zoom_in` / `app.zoom_out` / `app.zoom_reset` |
| Focus the source | `herdr-app do pane.focus --pane <p>` |
| **Trigger pluck exactly as a key does** | `herdr-app do pluck.copy` / `herdr-app do pluck.open` (delivers to the mod's `nextCommand()`) |
| Find the picker | `hc ls --json` — launch the picker with `--label pluck=picker` (add this to the mod's `hc run`) so it's easy to find |
| What the picker shows (text) | `hc read --json --lines 0 <picker>` |
| Picker size == source size | `hc get --json <picker>` vs `hc get --json <source>` |
| **Type a hint / Shift / Backspace / Esc** | `hc keys <picker> a`, `hc keys <picker> s d`, `hc keys <picker> esc`, `hc keys <picker> backspace`; Shift: spike which spelling works (`A` or `shift+a`) |
| Clipboard result | `pbpaste` (back up first: `pbpaste > clip.bak`, restore after: `pbcopy < clip.bak`) |
| Picker's own record | `<run-dir>/result.json` (keep run dirs in a debug mode, see below) |
| Toasts | `herdr-app shot` while the toast is visible; `herdr-app status` for errors |

**Isolation: decide in step 1 (spike), in this order of preference:**

1. **Private profile window.** `HERDR_APP_HOME=<tmp dir>` selects a different
   app home, and `status`/`shot`/`do` honor it. Spike whether a second
   window can run with its own home (own layout with the 0px block, own
   `keys.json`, the mod symlinked in) alongside the user's window. If yes,
   all automated verification runs there and never touches the user's
   window. Note `hc` terminals are shared with the user's core: only touch
   terminals the run created.
2. **The user's real window, with permission.** If (1) isn't possible,
   **ask the user before every verification session** (old feature note
   says so too). Work only in a dedicated `pluck-test` tab the run creates
   and closes; never invoke pluck on, or type into, the user's own
   terminals; back up and restore the clipboard; restore zoom/layout.

**Test-only hooks (needed to verify without side effects):**

- A **dry-run marker** for opening URLs: if `<appHome>/pluck-dev.json` has
  `{"dry_run_open": true}`, the mod writes the URL to `<run-dir>/opened.txt`
  instead of `cx.open_url`. Only meant for the private profile. Verify the
  real `cx.open_url` path once by hand (or with a `file://` URL to a temp
  page, with permission).
- `{"keep_run_dirs": true}` in the same file keeps `$TMPDIR/herdr-app-pluck/<nonce>/`
  (result.json, capture, logs) for evidence.
- These hooks must be inert when the file is absent.

**One drive, the standard recipe:**

```text
1. herdr-app status                       → no errors; pluck block in layout
2. create pluck-test tab running a fixture; wait for its end-of-fixture line
   (hc wait <t> --output end-of-fixture, or poll hc read)
3. herdr-app shot before.png ; hc read --json <source> > source.json
4. herdr-app do pluck.copy
5. poll hc ls for the pluck=picker label; hc get → size equals source
6. hc read --json <picker> > picker.json ; herdr-app shot picker.png
7. hc keys <picker> <hint>
8. picker gone from hc ls; pbpaste == expected; result.json matches
9. herdr-app shot after.png → pane shows its own terminal again
10. close the test tab, restore clipboard
```

**Wrap this in a helper.** In step 1, create a project skill
`.agents/skills/verify-herdr-app-pluck/` (symlink into `.claude/skills/`),
modelled on the old one: `SKILL.md`, `scripts/pv` with subcommands
(`up`, `doctor`, `seed <fixture>`, `split`, `zoom`, `invoke copy|open`,
`select <keys…>`, `shot <name>`, `down`), `fixtures/`, and
`features/README.md` mapping every journey below to commands and the
observable proof. Evidence goes to
`target/verify/<run-id>/evidence/NN-<action>.*` (source/picker reads,
screenshots, keys typed, clipboard, result.json) and survives `down`.
`doctor` checks: app reachable, no `status` errors, mod is this checkout's
`mod/` and `herdr-app check` passes, `mod/bin` binary is byte-identical to
`target/release/herdr-app-pluck`, test tab focused, no stray picker.

**Feature journeys** (each step in §8 adds its rows; keep the map current):

| Journey | Proof |
|---|---|
| copy, 1-letter hint | hints visible in `picker.json`; `pbpaste` = token; picker gone |
| copy, 2-letter hints (`many.txt`) | all hints 2 letters; partial first letter narrows (re-read screen); full hint copies |
| backspace | after first letter + backspace, screen equals the initial hinted screen |
| cancel (Esc, Ctrl-C) | clipboard unchanged; picker gone; pane restored |
| no matches | toast, no picker terminal ever shown |
| open (`pluck.open`) | only URLs hinted; `opened.txt` = URL (dry run) |
| Shift flip | copy+Shift on URL → opened; open+Shift → copied; copy+Shift on non-URL → copied |
| duplicates | same hint at every occurrence; copies the text once |
| custom / project patterns | private config + project `.herdr-pluck.toml` matches hinted and copied; `(?<match>)` copies capture only |
| shared config w/ `clipboard` key | no error toast; patterns still apply |
| bad config | error toast names the key; defaults used |
| OSC 8 | hint on visible text; copies hidden URL |
| soft wrap | long token across rows: hint shown, whole token copied |
| full screen (nvim, less, lazygit) | hints over the right cells; keys work despite nvim's kitty/mouse modes |
| splits, zoomed pane, app zoom ±1 | same as copy journey in each; exactness harness green |
| dimming | non-match text in dim color; text grid unchanged |
| remote terminal | toast "local terminals only"; no picker |
| layout terminal block (no pane) | toast "Focus a terminal pane first" |
| busy | second `do pluck.copy` while picker open does nothing |
| picker crash / show timeout | error toast; no orphan picker in `hc ls` |

**Proof standards** (from the old skill, still apply):

- Drive through `herdr-app do pluck.*` (the key path), never by running the
  binary directly, except for unit-level debugging.
- Capture both what the user saw (picker read + screenshot) *and* the side
  effect (clipboard / opened.txt / toast). Either alone proves nothing.
- Look at every screenshot you cite. Don't claim a visual check you didn't
  make.
- Known gap: `hc keys` types into the picker directly, so it doesn't prove
  the app routes real window key presses to the shown terminal. Ask the
  user for one manual key-press check per milestone.
- `herdr-app shot` returns the last drawn frame; it can be stale if the
  window isn't drawing. Re-shoot after a short wait if in doubt.
- `tab.new` without `--space` opens in the focused space, which may not be
  the one on screen: always pass `--space`.
- Don't override `XDG_CONFIG_HOME` for anything that talks to the app
  (`herdr-app` uses it to find the window socket). Point the picker at test
  configs with `--config`.

### 7.2 Exactness harness

Exactness is proven by an automated harness, not by eye. Build it in step 1,
before any hint drawing, on top of the §7.1 helper (`pv exact <fixture>`).

#### A. Text grid check (primary)
For each fixture: start it in a herdr-app pane, `hc read --json <source>`,
invoke the picker with `--no-dim` and hints disabled (or ignore hint cells),
`hc read --json <picker>`, compare **cell by cell**. Must be identical.

#### B. Pixel check
`herdr-app shot before.png`, invoke, `herdr-app shot after.png`. With
`--no-dim` and hints off: zero differing pixels in the pane. With hints on:
differences only inside hint/highlight cell rectangles (compute rects from
pane bounds in the screenshot; simplest is to diff and assert the diff mask's
bounding boxes align to the hint cells). With dim on: the pane's glyph
*shapes* match (e.g. compare a thresholded/binarized luminance mask).

### 7.3 Fixtures

Scripts that print deterministic content, end with an `end-of-fixture` line,
then sleep. They live in the verify skill's `fixtures/`:

- plain ASCII with every built-in token type (reuse herdr-pluck
  `.agents/skills/verify-herdr-pluck/fixtures/`)
- CJK wide chars, emoji incl. ZWJ sequences and VS16, combining marks, tabs
- lines that soft-wrap at the pane width; long URL across a wrap
- heavy 256-color/truecolor SGR, reverse video, colored backgrounds
- OSC 8 named links (`printf '\e]8;;https://example.com\e\\GitHub\e]8;;\e\\'`)
- full-screen programs: `vim`/`nvim`, `less`, `lazygit` (alternate screen)
- a program with kitty keyboard protocol / mouse mode on (nvim does)

…each run in: single pane, split right, split down, zoomed pane, and at app
zoom levels 0, +1, −1 (`app.zoom_in`/`app.zoom_out`/`app.zoom_reset`).

### 7.4 Unit tests

Rust, `cargo test`: SGR rewriter, sanitizer, decoder
cells/widths/OSC 8, matcher (port old tests), hint assignment, result JSON,
config (incl. ignored `clipboard`). JS: keep logic thin; if it grows, add
`node --test` tests like link-hints does.

### 7.5 Manual pass

By the user at each milestone: real key presses (the `hc keys` gap), OSC 8
behavior (decision 9 is provisional) and the dim look.

### 7.6 Definition of done (every task)

1. `cargo fmt --all -- --check`, `cargo clippy --all-targets`, `cargo test`.
2. `herdr-app check mod/` passes and `herdr-app status` shows no errors.
3. The §7.1 journeys touched by the change are re-driven, with evidence
   saved and screenshots actually looked at; the feature map is updated.
4. The exactness harness (§7.2) is green for anything that touches capture,
   replay, dimming or drawing.
5. Report evidence paths to the user. If something couldn't be verified (for
   example, the user declined real-window access), say so explicitly.

**Safety:** verification drives a real herdr-app. Prefer the private profile.
Otherwise ask first, use only tabs/terminals the run creates and closes, never
send keys to the user's own terminals, never stop/restart the app or core,
and restore clipboard, zoom and layout afterwards.

## 8. Build order

Each step ends green on §7.6 (definition of done) before the next starts:
its new journeys are added to the verify skill's feature map and driven with
evidence, and earlier journeys it could affect are re-driven.

0. **Scaffold.** `git init`; `Cargo.toml` (crate `herdr-app-pluck`, edition
   2021, deps from herdr-pluck: anyhow, clap, crossterm, regex, serde,
   serde_json, thiserror, toml, unicode-width); `mod/`; `scripts/install.sh`
   (`cargo build --release`, copy to `mod/bin/`, refuse to clobber
   non-symlinks, run `herdr-app check mod`); `AGENTS.md` with the ground
   rules from §0, the definition of done from §7.6, and a pointer to the
   verify skill; README stub.
1. **Verification helper + capture/replay spike + harness.**
   - First the isolation spike (§7.1: private profile window or not; Shift
     spelling for `hc keys`; does `herdr-app do pluck.copy` reach the mod).
     Report the outcome to the user before going further.
   - Build the verify skill and `pv` helper (§7.1) with `up`/`doctor`/
     `seed`/`invoke`/`select`/`shot`/`down`.
   - Picker that only captures, waits for size, writes the sanitized capture
     verbatim, waits for Esc. Mod wiring enough to launch/show it (copy + no
     hints). Harness A and B with `--no-dim`.
   - **Gate: zero differences on every fixture and layout.** If this can't be
     achieved, stop and report to the user: decision 3 is invalid.
2. **Decoder + matching + hints.** Choose decoder (§4.3), port patterns/hints,
   draw hints and highlights (§4.5), single-letter and two-letter hints.
   Result file. Harness B with hints.
3. **Mod actions.** Native copy, toasts, error paths (no matches, crash,
   timeout, remote, no pane), busy flag.
4. **Dimming.** SGR rewriter + theme colors. Harness: text grid identical,
   glyph masks identical.
5. **`pluck.open` + Shift flip.**
6. **OSC 8, project patterns, backspace, soft-wrap joining, config warnings.**
7. **Wiring + docs.** README (install, keys, layout block, scrollback
   limitation, config sharing). Then, *with the user's go-ahead*, dotfiles:
   replace the old `pluck` symlink with a `mise.toml` entry pointing at this
   repo's `mod/`, add keys to `dots/.config/herdr-app/keys.json`, add the 0px
   block to `dots/.config/herdr-app/layouts/neverforest.json`.

## 9. Later (not in the tracer bullet)

- **Remote terminals.** Capture works over `hc attach --observe machine/t7`
  (link-hints relies on it). Open question for a spike: can `terminal.show`
  put a *local* picker terminal into a remote space's pane? If yes, remote is
  nearly free. If no: run the picker with `hc run --on <machine>`, ship the
  binary with `hc put`, cross-compile for the NAS. Clipboard needs nothing:
  the native call already writes to the machine running the app. Do not port
  herdr-pluck's OSC 52 clipboard work.
- Blend backgrounds toward the terminal background (~30%) instead of
  stripping.
- Prebuilt release binaries (herdr-pluck's GitHub Actions flow).

## 10. What to reuse from herdr-pluck

`~/code/rmarganti/herdr-pluck` (v0.3.2):

| Keep / adapt | Drop |
|---|---|
| `src/patterns.rs` (all built-ins, priorities, `(?<match>)` capture, trailing-punctuation trim) | `src/herdr/*` (TUI plugin protocol, layout recreation) |
| `src/hints.rs` | `src/clipboard/*`, `src/url_opener/*` |
| `src/config.rs` (+ ignore `clipboard`) | `src/viewport.rs`, re-wrap of `hc read` lines |
| `src/app/hc.rs` (`TerminalFacts`, `run`, `run_background`, `kill`) | `herdr-plugin.toml`, `scripts/install-binary.sh` TUI parts |
| `src/picker/input/*` (raw mode, cursor guard, key events; add Backspace + Shift) | `src/renderer.rs` text composition (replaced by verbatim replay) |
| `show_until_sized` logic from `src/app/mod.rs` (move into the mod) | |
| `.agents/skills/verify-herdr-pluck/fixtures` and feature notes | |

From link-hints (`~/.cache/checkouts/github.com/diegoeche/herdr-link-hints`):
`snapshot.py` (observer lifecycle and limits), `terminal_grid.py` (what a
minimal decoder must handle), `links.js` (URL trimming of prose punctuation and
unbalanced brackets, OSC 8 regex), `RECIPE.md` (style of a rebuild spec).
