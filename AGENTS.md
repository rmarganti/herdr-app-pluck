# herdr-app-pluck: agent notes

Keyboard hints over the focused Herdr app pane: copy a visible token or open a visible
URL. `PLAN.md` is the design and hand-off; read it before changing behavior.

- `src/`: the Rust picker (`herdr-app-pluck pick`), run in its own herdr-core terminal.
- `mod/`: the JS mod (`pluck`), symlinked as `~/.config/herdr-app/mods/pluck`.
- `scripts/install.sh`: build, copy the binary into `mod/bin/`, `herdr-app check mod`.

## Ground rules

- **Placement is exact.** When pluck opens, no character on screen may move. Hints and
  colors change; glyph positions never do. A build that shifts text is broken. The only
  accepted exception: a scrolled-up pane jumps to its live screen.
- The picker replays the raw `hc attach --observe` capture verbatim (sanitized for input
  modes, SGR-rewritten for dimming). Our decoder (libghostty-vt, like herdr-core) only
  decides where hints go. Never redraw cells from the decoder; never print a wide or
  combining character ourselves.
- When `PLAN.md` is silent, copy herdr-pluck's behavior (`~/code/rmarganti/herdr-pluck`).
- Config is shared with herdr-pluck (`~/.config/herdr-pluck/config.toml`). Don't add a
  key the old plugin doesn't know without asking the user first: it rejects unknown keys.
- Ask the user in plain chat text, never with a question tool.
- Verify every feature in the real app yourself before calling it done.

## Definition of done (every task)

1. `cargo fmt --all -- --check`, `cargo clippy --all-targets`, `cargo test`.
2. `herdr-app check mod/` passes and `herdr-app status` shows no errors.
3. The journeys touched by the change are re-driven with the verify skill, evidence saved,
   screenshots actually looked at, and the feature map updated.
4. The exactness harness (`pv exact`) is green for anything touching capture, replay,
   dimming or drawing.
5. Report evidence paths. Say explicitly what couldn't be verified.

## Verifying

Use the project skill `.agents/skills/verify-herdr-app-pluck/` (`SKILL.md`, helper
`scripts/pv`). It runs a private Herdr window with its own herdr-core session, so the
person's window, core and terminals are never touched.

## Safety

- Delete only paths you made, one `rm` per path, written out literally, after an `ls` of
  the target. No `rm -rf` on computed paths.
- Never stop, restart or `hc server status` the person's core; `hc server status` starts
  a core that isn't running.
- Strip `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET` and `HC_SCOPE` from anything aimed at the
  private session (`pv` does), or `hc` may reach the person's core.
