# Feature map

Every journey, how to drive it with `pv`, and what proves it. **Status** is the last run
that proved it (evidence in `target/verify/<run>/evidence/NN-*`), or why it isn't proven.

Run `20261103-233055` was `pv up --real` (the person's window, with permission) on
herdr-app 0.1.12 / hc 0.1.17, through the `pluck-dev` mod pane, except drive 46, which
went through the layout's own `pluck` copy.

## Choosing journeys

| You changed | Re-run |
|---|---|
| capture, sanitize, replay, dimming, drawing (`capture.rs`, `sanitize.rs`, `dim.rs`, `render.rs`, `picker.rs`) | every `exact` row, copy, two-letter, full screen, dimming |
| decoder / matching (`grid.rs`, `targets.rs`, `patterns.rs`, `hints.rs`) | copy, two-letter, duplicates, unicode, soft wrap, OSC 8, `exact --hints` |
| config (`config.rs`) | custom/project patterns, shared config, bad config |
| keys (`input.rs`) | copy, two-letter, backspace, cancel, Shift flip, full screen |
| the mod (`mod/main.js`) | copy, open, cancel, no matches, busy, crash, resize; then reload the layout (see SKILL.md) |

## Journeys

| Journey | Drive | Proof | Status |
|---|---|---|---|
| copy, 1-letter hint | `seed tokens.txt; invoke copy; select d` | `picker.json` hints; clipboard `src/picker/session.rs`; result picked/copy; toast "Copied …"; pane restored (`after.png`) | ✅ 233055 #02 |
| Git paths exclude terminal padding | Seed `modified:   dots/.bash_aliases` and `+++ b/docs/my notes.md`; `invoke copy; select a`, then `invoke copy; select s`; `exact KEEP` | clipboard and result contain exactly each path, preserving internal spaces; screenshot inspected; exact replay has 0 differing pixels | ✅ private run `20261110-094333` #01–03 |
| copy, 2-letter hints | `seed many.txt; invoke copy; select s; select a` | all hints 2 letters; after `s` only `s?` remain, others restored (`after-keys-1`); clipboard `10.0.0.27` | ✅ #06 |
| backspace | `invoke copy; select s; select backspace` | `after-keys-2.picker.json` text == `picker.json` text | ✅ #07 |
| cancel (Esc, Ctrl-C) | `select esc` / `select ctrl+c` | result cancelled; clipboard sentinel unchanged; picker gone | ✅ #07, #08 |
| letter that starts no hint | 31 values, `select f` | ignored, picker stays | ✅ #37 |
| no matches | `seed --cmd sh -c 'echo plain words only; …'; invoke copy` | no picker ever shown; result no_matches | ✅ #16 |
| open (`pluck.open`) | `seed tokens.txt; invoke open; select a` | only the 3 openable URLs hinted; `opened.txt` = URL; clipboard unchanged | ✅ #09 |
| Shift flip | `hc keys` spelling is **`shift+a`** (`A` arrives unshifted) | open+Shift → copied (#13); copy+Shift on URL → opened (#14); copy+Shift on path → copied (#15) | ✅ #13–15 (#10–12 used `A`: void) |
| duplicates | `seed tokens.txt; invoke copy` | both `session.rs` rows show `d`; copied once | ✅ #02 |
| custom / project patterns | `seed custom-patterns.txt; invoke copy` | `PLK-42` (project), `abc123` (`match` capture, #17), `GLB-7`, `build-1a2b3c4` beats git-sha (#18); `prec=alpha` copies whole, as herdr-pluck does (longer span wins at equal priority) | ✅ #17–19 |
| shared config with `clipboard` | `config global-config-shared.toml` | no warning; global patterns apply | ✅ #19 |
| bad config | `config bad-config.toml` | toast "config.toml line 2: unknown field `foo`; using defaults"; global patterns gone, project ones stay | ✅ #21 |
| soft wrap | `seed wrap.sh; invoke copy` | one hint per wrapped token; whole URL / path copied | ✅ #23, #24 (needed `hc read` wrap marks: the repaint ends every row with CR LF) |
| unicode | `exact unicode.sh --hints`, `exact KEEP` | hints right after CJK, 🔥, ZWJ family, ❤️, 🇯🇵, 👍🏽, tabs, ZWSP; `café/…` gets its hint only; strict 0 px | ✅ #25–27 |
| colors / reverse / backgrounds | `exact colors.sh`, `exact KEEP --dim` | strict 0 px; dim: text identical, glyphs differ only on rows with backgrounds (stripped by design) | ✅ #28–30 |
| OSC 8 | `seed osc8.sh; invoke copy` | hint on `GitHub`, copies hidden URL | ❌ **blocked upstream**: hc 0.1.17's observer repaint drops OSC 8 (no hint on `GitHub`, #31). Decoder/matcher support unit-tested |
| full screen: nvim | `seed --cmd nvim -u NONE fixtures/fullscreen.txt --wait getting-started; exact KEEP; invoke copy` | keys work despite kitty (`CSI =3;1u`), mouse, `?1h`; text identical; pixels differ only at nvim's cursor and its last row's background (the observer doesn't repaint blank rows that carry only a background) | ✅ #32, #33 (with that exception) |
| full screen: less | `seed --cmd less fixtures/fullscreen.txt …` | text identical; only the cursor cell differs; copy works | ✅ #34, #35 |
| split right | `split many.txt right; exact KEEP; invoke copy` | 37x40; strict 0 px; two-letter copy | ✅ #36 |
| app zoom ±1 | `appzoom in; exact KEEP; …; appzoom out; …` | 33x35 and 43x44: strict 0 px; copy works | ✅ #38–41 |
| zoomed pane | through the layout copy (a zoom hides the mod pane) | 163x40: strict 0 px; copy `10.0.0.27` | ✅ #45, #46 |
| resize while open | app zoom with a picker up | result cancelled "The pane changed size, so pluck closed." | ✅ #37 |
| dimming | `exact tokens.txt --dim` | text identical; glyph masks identical (1 px antialias tolerance); non-match text `#77817d` | ✅ #05 |
| busy | `invoke copy` then `herdr-app do <mod>.copy` again | one picker; nothing queued after | ✅ #08 |
| picker crash | `hc kill <picker>` mid-pick | toast "The picker exited without a result."; no orphan | ✅ #42 |
| show timeout | needs a pane that never shows the picker | error toast, picker killed | not driven (code path shared with crash; old herdr-pluck journey) |
| remote terminal | needs a remote space | toast "local terminals only" | not driven |
| layout terminal block (no pane) | needs a layout terminal block | toast "Focus a terminal pane first" | not driven |
| real key press | person presses `cmd-shift-f` | `hc keys` can't prove the app routes window keys | **ask the person** |
