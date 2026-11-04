use crate::color::Rgb;
use crate::model::Action;
use crate::picker::{self, PickOptions};
use crate::render::HintStyle;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "herdr-app-pluck",
    version,
    about = "Keyboard hints over a Herdr app pane"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Capture a terminal, show hints over it, and write the pick to <run-dir>/result.json.
    /// Meant to run in its own herdr-core terminal shown in the source's pane.
    Pick(PickArgs),
}

#[derive(Debug, clap::Args)]
struct PickArgs {
    /// The terminal to pluck from (`t7`).
    #[arg(long)]
    source: String,
    #[arg(long, value_enum)]
    action: Action,
    /// Where `ready` and `result.json` go.
    #[arg(long)]
    run_dir: PathBuf,
    /// Global config (default: $XDG_CONFIG_HOME/herdr-pluck/config.toml).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Color for everything that isn't a match.
    #[arg(long, default_value = "#6c7086")]
    dim_color: Rgb,
    #[arg(long, default_value = "#11111b")]
    hint_fg: Rgb,
    #[arg(long, default_value = "#f9e2af")]
    hint_bg: Rgb,
    #[arg(long, default_value = "#cdd6f4")]
    match_fg: Rgb,
    /// The hc binary (default: PATH, then ~/.local/bin, Homebrew).
    #[arg(long)]
    hc: Option<PathBuf>,
    /// Debug: replay the capture without dimming.
    #[arg(long)]
    no_dim: bool,
    /// Debug: draw no hints; Esc closes (exactness harness).
    #[arg(long)]
    no_hints: bool,
    /// Debug: save the raw capture bytes.
    #[arg(long)]
    capture_out: Option<PathBuf>,
}

pub fn run() -> i32 {
    let cli = Cli::parse();
    match cli.command {
        Command::Pick(args) => {
            if let Some(hc) = args.hc {
                crate::hc::set_program(hc);
            }
            picker::run(&PickOptions {
                source: args.source,
                action: args.action,
                run_dir: args.run_dir,
                config: args.config,
                dim_color: args.dim_color,
                style: HintStyle {
                    hint_fg: args.hint_fg,
                    hint_bg: args.hint_bg,
                    match_fg: args.match_fg,
                },
                no_dim: args.no_dim,
                no_hints: args.no_hints,
                capture_out: args.capture_out,
            })
        }
    }
}
