---
name: verify-herdr-app-pluck
description: Live, end-to-end verification of herdr-app-pluck in a real Herdr app window. Starts a private Herdr window with its own app home and herdr-core session, seeds fixture tabs, presses pluck.copy / pluck.open through `herdr-app do`, types hints with `hc keys`, and saves screenshots, screen reads, clipboard and result files as evidence. Includes the exactness harness (`pv exact`). Use after changing the picker, the mod, patterns, hints, config, dimming or drawing, or whenever a task needs proof that pluck works in the app and not just in `cargo test`.
---

# Verify herdr-app-pluck

One helper, run from the repo root:

```bash
PV=.agents/skills/verify-herdr-app-pluck/scripts/pv
$PV --help
```

Needs `herdr-app`, `hc`, `jq`, `python3`, `cargo`. macOS.

## Two modes

**`pv up --real`: the person's own window.** Ask first, every session. pv then:

- links this checkout's `mod/` as `mods/pluck-dev` (refuses if that name is taken by
  something else) and, per test tab, splits a `pluck-dev` **mod pane** into it with props
  `{binary, devFile}`. That puts the mod in the shown layout without touching the
  person's layout, keys or `mods/pluck`. `invoke` sends `herdr-app do pluck-dev.<action>`;
- works only in a `pluck-test` tab it creates in the shown space, and closes it;
- keeps test hooks in `<run>/dev.json` (the mod pane's `devFile`), never in the person's home;
- `down`: closes the test tab, re-selects the tab that was shown, undoes app-zoom steps,
  removes the `pluck-dev` link, restores the clipboard.

Never type into, invoke on, or kill a terminal pv didn't make. Pickers are found as new
addresses since the invoke, labelled `pluck=picker`.

**`pv up`: a private window.** Own app home (`$TMPDIR/hapv.XXXXXX`, layout `pluck` with
a 0px pluck block, `shared.json`/`update-notes.json` seeded so no dialogs open) and own
herdr-core session (`HC_SESSION=pv<run-id>`): nothing is shared with the person but the
clipboard. **It only works while the window is uncovered**: a covered macOS window
doesn't draw, so the app never lays out tabs or applies `terminal.show` (symptom: the
source stays 120x40 and `after.png`'s clock is frozen). Prefer `--real` unless the person
can keep it visible.

Both: `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET`, `HC_SCOPE` are stripped from every call
(an agent inside a Herdr terminal inherits them). The clipboard is saved at `up` and
restored at `down`.

## Lifecycle

```bash
$PV up --real         # scripts/install.sh + pngdiff, the person's window, doctor
$PV up [--no-build]   # ... or a private window
$PV doctor            # read-only: window, errors, layout, mod link, binary == target/release,
                      # private core, source focused, no stray picker
$PV down              # quit window, end the private session's terminals, stop that core,
                      # restore clipboard, delete only what this run made (each path printed)
```

`down` keeps `target/verify/<run-id>/evidence/`. Run it after failures too.
Never run `hc server status` for the private session after `down`: it starts the core.

## Driving

```bash
$PV seed tokens.txt                 # new "pluck-test" tab running fixtures/run.sh tokens.txt
$PV seed --cmd nvim fixtures/... --wait getting-started   # any program
$PV split many.txt right            # split the source; the new pane is the source
$PV zoom              # private mode only: a zoom hides the pluck-dev mod pane
$PV appzoom in|out|reset
$PV config bad-config.toml          # the picker's global config (via pluck-dev.json "config")
$PV dev '{"dry_run_open": true}'    # test hooks in pluck-dev.json (null removes a key)
$PV invoke copy                     # herdr-app do pluck.copy, exactly what the key does
$PV select a                        # hc keys <picker> a; waits for the picker to close
$PV select s ; $PV select backspace # partial input: saves the screen after each step
$PV select shift+a                  # Shift: hc keys needs `shift+a`; `A` arrives unshifted
$PV shot name
```

Each `invoke` is a drive `NN-<action>`; files in `evidence/`:

| File | Proves |
|---|---|
| `NN-x.source.json`, `.source-size.txt`, `.before.png` | what the user saw before |
| `NN-x.picker.json`, `.picker-size.txt`, `.picker.png` | the hinted screen, at the pane's size |
| `NN-x.after-keys-K.*` | the screen after partial input |
| `NN-x.keys.txt` | keys typed |
| `NN-x.result.json` | the picker's own record (`picked`/`cancelled`/`no_matches`/`error`) |
| `NN-x.clipboard` | clipboard after the drive |
| `NN-x.opened.txt` | URL the mod would have opened (dry run) |
| `NN-x.status.json`, `.after.png` | app status and frame after (toasts, pane restored) |
| `NN-x.capture.bin` | the raw observer capture |
| `NN-x.exact.json` | exactness verdict |

Test hooks, `<app home>/pluck-dev.json` (inert when absent; `up` writes it):
`dry_run_open`, `keep_run_dirs`, `capture_out`, `config`, `no_dim`, `no_hints`.

## Exactness harness

`exact` first calibrates (`pv calibrate`): it shows a solid magenta terminal in the source
pane, screenshots it and crops every pixel diff to that box (the pane's cell area), so
live window chrome (agent spinners, clocks) can't fail it. It re-calibrates after
`split`, `zoom` and `appzoom`.

```bash
$PV exact tokens.txt            # A+B: --no-dim --no-hints: text identical, zero pixels differ
$PV exact tokens.txt --dim      # dim on: text identical, glyph masks compared
$PV exact tokens.txt --hints    # hints on: text differs only by hint letters, row lengths equal
$PV exact --cmd nvim -u NONE fixtures/fullscreen.txt
$PV exact KEEP                  # re-run on the current source (after split/zoom/appzoom)
```

- **Text grid (A)**: `hc read` of source vs picker, row by row.
- **Pixels (B)**: `herdr-app shot` before vs with the picker shown, cropped to the pane.
  Zero differing pixels with `--no-dim --no-hints`. With `--dim`, `pngdiff --glyphs`
  compares ink masks with a 1 px antialias tolerance (a moved glyph moves ~20 px), and
  must be zero; colored backgrounds are stripped by design, so rows with backgrounds
  differ there (`colors.sh`).
- Known, accepted pixel differences: a program's **visible cursor** (the picker hides
  it: nvim, less), and nvim's **last row background**: herdr-core's observer doesn't
  repaint a trailing row of blank cells that only carry a background.
- Fixtures hide the cursor (`run.sh`): the picker hides it, and a cursor is not text.

## Gotchas

- **Stale mod copies.** Re-pointing `mods/<name>` at a different folder doesn't reload a
  running copy; it keeps the old code (seen: the layout's `pluck` copy kept starting the
  old `herdr-pluck` after the link moved). Switch the layout away and back
  (`herdr-app do layout.switch --name main`, then the person's layout), then check the
  picker's argv (`hc get <picker>`).
- A picker terminal gets herdr-core's environment, not a login shell: `hc` may not be on
  its PATH. The mod passes `--hc <path>`; the picker also looks in `~/.local/bin`.
- `herdr-app ls` lists terminal panes only, never mod panes.
- Never `hc server status` a stopped private session: it starts the core again.

## Proof standards

- Drive through `$PV invoke` (`herdr-app do pluck.*`, the key path), never by running the
  binary directly, except for unit-level debugging.
- Capture both what the user saw (picker read + screenshot) *and* the side effect
  (clipboard / opened.txt / toast). Either alone proves nothing.
- Look at every screenshot you cite (`sips -Z 1000 in.png --out small.png`, then read it).
- `hc keys` types into the picker directly; it doesn't prove the app routes window keys
  to the shown terminal. Ask the person for one manual key press per milestone.
- `herdr-app shot` is the last drawn frame; if the clock in it is old, the window isn't
  drawing (see Isolation).

## Feature map

[`features/README.md`](features/README.md): every journey, its commands, its proof.

## Fixtures

`fixtures/`: `tokens.txt` (every built-in type, a duplicate, trailing period),
`many.txt` (30 IPs: two-letter hints), `no-urls.txt`, `custom-patterns.txt`,
`unicode.sh` (CJK, ZWJ/VS16/flag/skin-tone emoji, combining marks, tabs, ZWSP),
`colors.sh` (16/256/truecolor, backgrounds, reverse, colon SGR), `wrap.sh` (soft-wrapped
URL and path), `osc8.sh` (named links), `fullscreen.txt` (for nvim/less);
configs `global-config.toml`, `global-config-shared.toml` (with herdr-pluck's
`clipboard`), `bad-config.toml`, `project-config.toml` (installed as the fixture
project's `.herdr-pluck.toml`). Each prints `end-of-fixture` last.
