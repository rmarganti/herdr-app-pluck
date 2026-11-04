// Pluck for the Herdr app: keyboard hints over the focused pane to copy a visible token
// or open a visible URL. Keys: "pluck.copy" and "pluck.open" in keys.json.
//
// The herdr-app-pluck binary (bin/, built by scripts/install.sh) runs in its own
// herdr-core terminal: it captures the pane's terminal, finds tokens, and once it's ready
// this mod shows it in the pane (terminal.show). It replays the pane's screen exactly,
// draws hints, reads keys and writes result.json; when it exits the pane gets its own
// terminal back. Copying and opening happen here, natively.
//
// Draws nothing; it only needs a 0px block in the layout so key commands reach it.

import { View, div } from "gpui-kit";
import { appHome, exec, focused, hc, nextCommand, notify, props, readTextFile, run, theme, watchFolder } from "herdr";

const TITLE = "Pluck";
const COMMANDS = { copy: "copy", open: "open", "open-url": "open" };

const READY_TIMEOUT_MS = 10000; // capture waits up to 5 s for a first byte
const SHOW_TIMEOUT_MS = 3000;
const SHOW_RETRY_MS = 250;
const POLL_MS = 25;
const ALIVE_CHECK_MS = 1000;

let busy = false;

function toast(body, level = "info", timeout) {
  notify({ title: TITLE, body, level, ...(timeout ? { timeout } : {}) });
}

function short(text, max = 60) {
  const flat = text.replace(/\s+/g, " ");
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat;
}

async function readJson(path) {
  const text = await readTextFile(path).catch(() => null);
  if (text == null) return null;
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

async function hcJson(args) {
  const r = await hc(args);
  if (r.code !== 0) return null;
  try {
    return JSON.parse(r.stdout);
  } catch {
    return null;
  }
}

/** A live terminal's facts, or null once it has exited or is gone. */
async function liveFacts(terminal) {
  const facts = await hcJson(["get", "--json", terminal]);
  return facts && facts.state === "live" ? facts : null;
}

/** Test-only hooks: <appHome>/pluck-dev.json, or the file a test copy's props name
 *  (`devFile`). Inert when the file is absent.
 *  dry_run_open: write the URL to <run-dir>/opened.txt instead of opening it.
 *  keep_run_dirs: keep $TMPDIR/herdr-app-pluck/<nonce>/ (result.json, capture) as evidence.
 *  config: global config path for the picker. capture_out: save capture.bin.
 *  no_dim, no_hints: exactness harness (replay only). */
async function devHooks(options) {
  return (await readJson(options.devFile ?? `${appHome()}/pluck-dev.json`)) ?? {};
}

/** Where this copy finds its binary and test hooks. A layout block gets the defaults;
 *  a test copy (a mod pane from the verify skill) passes `binary` and `devFile` props. */
function copyOptions() {
  const p = props() ?? {};
  return {
    binary: typeof p.binary === "string" ? p.binary : `${appHome()}/mods/pluck/bin/herdr-app-pluck`,
    devFile: typeof p.devFile === "string" ? p.devFile : undefined,
  };
}

const HEX = /^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/;

function colorArgs() {
  const t = theme() ?? {};
  const args = [];
  const add = (flag, value) => {
    if (typeof value === "string" && HEX.test(value)) args.push(flag, value);
  };
  add("--dim-color", t.dim);
  add("--hint-fg", t.accent_foreground);
  add("--hint-bg", t.accent);
  add("--match-fg", t.foreground);
  return args;
}

/** Waits until something changes in `dir` or `ms` passes, whichever is first. */
function makeWaiter(dir, cx) {
  let watch = null;
  return (ms) => {
    if (!watch) {
      watch = watchFolder(dir).then(
        () => { watch = null; },
        () => { watch = null; },
      );
    }
    return Promise.race([watch, cx.sleep(ms)]);
  };
}

async function pluck(action, cx, options) {
  const f = focused();
  const source = f?.shown ?? f?.terminal;
  // A mod pane has no terminal; a layout terminal block has no pane to show the picker in.
  if (!f?.pane || !source) {
    toast("Focus a terminal pane first.", "warning");
    return;
  }
  if (source.includes("/")) {
    toast("Pluck only works on local terminals for now.", "warning");
    return;
  }
  const pane = f.pane;
  const binary = options.binary;
  if ((await exec("test", ["-x", binary]).catch(() => ({ code: 1 }))).code !== 0) {
    toast("The pluck binary is missing. Run scripts/install.sh in herdr-app-pluck.", "error");
    return;
  }
  const dev = await devHooks(options);

  const sourceFacts = await liveFacts(source);
  if (!sourceFacts) {
    toast(`Terminal ${source} isn't running.`, "error");
    return;
  }

  const mk = await exec("/bin/sh", [
    "-c",
    'd="${TMPDIR:-/tmp}/herdr-app-pluck" && mkdir -p "$d" && mktemp -d "$d/XXXXXXXX"',
  ]);
  const runDir = mk.stdout.trim();
  if (mk.code !== 0 || !runDir) {
    toast(`Could not make a run folder: ${mk.stderr.trim()}`, "error");
    return;
  }

  let picker = null;
  try {
    const args = ["pick", "--source", source, "--action", action, "--run-dir", runDir, ...colorArgs()];
    if (typeof dev.config === "string") args.push("--config", dev.config);
    // The picker's terminal gets herdr-core's environment, whose PATH may not have hc.
    const which = await exec("/bin/sh", ["-c", "command -v hc"]).catch(() => null);
    if (which?.code === 0 && which.stdout.trim().startsWith("/")) args.push("--hc", which.stdout.trim());
    if (dev.capture_out) args.push("--capture-out", `${runDir}/capture.bin`);
    if (dev.no_dim) args.push("--no-dim");
    if (dev.no_hints) args.push("--no-hints");
    const runArgs = ["run", "--bg", "--json", "--rm", "--no-parent", "--no-notify", "--label", "pluck=picker"];
    const cwd = f.cwd ?? sourceFacts.foreground?.cwd ?? sourceFacts.cwd;
    if (cwd) runArgs.push("--cwd", cwd);
    const launched = await hcJson([...runArgs, "--", binary, ...args]);
    picker = launched?.address;
    if (!picker) {
      toast("Could not start the picker.", "error");
      return;
    }

    const result = await waitForPicker(cx, runDir, picker, sourceFacts, pane);
    await act(cx, result, runDir, dev);
  } finally {
    if (picker) await hc(["kill", picker]).catch(() => {});
    if (!dev.keep_run_dirs) await exec("rm", ["-rf", runDir]).catch(() => {});
  }
}

/** Launch → ready → shown at the pane's size → result. Returns a result object. */
async function waitForPicker(cx, runDir, picker, sourceFacts, pane) {
  const wait = makeWaiter(runDir, cx);
  const resultPath = `${runDir}/result.json`;
  let warned = false;
  const warn = (record) => {
    if (!warned && record?.warnings?.length) {
      warned = true;
      toast(record.warnings.join("\n"), "warning", 8000);
    }
  };

  // 1. Captured and matched (or finished early: no matches, error).
  const started = Date.now();
  let lastAlive = Date.now();
  for (;;) {
    const result = await readJson(resultPath);
    if (result) {
      warn(result);
      return result;
    }
    const ready = await readJson(`${runDir}/ready`);
    if (ready) {
      warn(ready);
      break;
    }
    if (Date.now() - lastAlive > ALIVE_CHECK_MS) {
      lastAlive = Date.now();
      if (!(await liveFacts(picker))) return (await readJson(resultPath)) ?? gone();
    }
    if (Date.now() - started > READY_TIMEOUT_MS) {
      return { outcome: "error", message: "The picker took too long to read the pane." };
    }
    await wait(POLL_MS * 2);
  }

  // 2. Show it in the pane; re-send until it takes the pane's size. The app acknowledges
  //    a show that arrives before it has heard of a just-started terminal but shows nothing.
  const showStarted = Date.now();
  for (;;) {
    run("terminal.show", { terminal: picker, pane });
    const attempt = Date.now();
    let sized = false;
    while (Date.now() - attempt < SHOW_RETRY_MS) {
      const facts = await liveFacts(picker);
      if (!facts) return (await readJson(resultPath)) ?? gone();
      if (facts.cols === sourceFacts.cols && facts.rows === sourceFacts.rows) {
        sized = true;
        break;
      }
      await cx.sleep(POLL_MS);
    }
    if (sized) break;
    if (Date.now() - showStarted > SHOW_TIMEOUT_MS) {
      return { outcome: "error", message: `The app didn't show the picker in pane ${pane}.` };
    }
  }

  // 3. The user picks. No time limit: they may think as long as they like.
  lastAlive = Date.now();
  for (;;) {
    const result = await readJson(resultPath);
    if (result) {
      warn(result);
      return result;
    }
    if (Date.now() - lastAlive > ALIVE_CHECK_MS) {
      lastAlive = Date.now();
      if (!(await liveFacts(picker))) return (await readJson(resultPath)) ?? gone();
    }
    await wait(250);
  }
}

function gone() {
  return { outcome: "error", message: "The picker exited without a result." };
}

async function act(cx, result, runDir, dev) {
  switch (result.outcome) {
    case "picked":
      if (result.action === "open") {
        await openUrl(cx, result.text, runDir, dev);
      } else {
        cx.write_to_clipboard(result.text);
        toast(`Copied ${short(result.text)}`, "success", 2500);
      }
      return;
    case "cancelled":
      if (result.message) toast(result.message, "info");
      return;
    case "no_matches":
      toast("No matches.", "info", 2500);
      return;
    case "error":
    default:
      toast(result.message ?? "Something went wrong.", "error");
  }
}

async function openUrl(cx, url, runDir, dev) {
  if (dev.dry_run_open) {
    await exec("/bin/sh", ["-c", 'printf "%s" "$1" > "$2"', "sh", url, `${runDir}/opened.txt`]);
    return;
  }
  if (/^https?:\/\//i.test(url)) {
    cx.open_url(url);
    return;
  }
  // cx.open_url takes only http(s); hand file:// URLs to the system opener.
  const opener = (await exec("test", ["-x", "/usr/bin/open"]).catch(() => ({ code: 1 }))).code === 0 ? "open" : "xdg-open";
  const r = await exec(opener, [url]).catch((e) => ({ code: 1, stderr: String(e) }));
  if (r.code !== 0) toast(`Could not open ${short(url)}: ${(r.stderr ?? "").trim()}`, "error");
}

export default class Pluck extends View {
  init(_props, cx) {
    const options = copyOptions();
    cx.spawn(async (cx) => {
      for (;;) {
        const command = await nextCommand();
        const action = COMMANDS[command];
        // One at a time: a second key while the picker is up does nothing.
        if (action && !busy) {
          busy = true;
          cx.spawn(async (cx) => {
            try {
              await pluck(action, cx, options);
            } catch (error) {
              toast(`Pluck failed: ${error}`, "error");
            } finally {
              busy = false;
            }
          });
        }
        // Using cx lets a hot reload end this loop; without it the old build keeps the keys.
        cx.notify();
      }
    });
  }

  render() {
    return div();
  }
}
