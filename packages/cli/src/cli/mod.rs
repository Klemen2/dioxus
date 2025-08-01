pub(crate) mod autoformat;
pub(crate) mod build;
pub(crate) mod build_assets;
pub(crate) mod bundle;
pub(crate) mod check;
pub(crate) mod config;
pub(crate) mod create;
pub(crate) mod doctor;
pub(crate) mod init;
pub(crate) mod link;
pub(crate) mod platform_override;
pub(crate) mod run;
pub(crate) mod serve;
pub(crate) mod target;
pub(crate) mod translate;
pub(crate) mod update;
pub(crate) mod verbosity;
pub(crate) mod winres;

pub(crate) use build::*;
pub(crate) use serve::*;
pub(crate) use target::*;
pub(crate) use verbosity::*;

use crate::platform_override::CommandWithPlatformOverrides;
use crate::{error::Result, Error, StructuredOutput};
use clap::builder::styling::{AnsiColor, Effects, Style, Styles};
use clap::{Parser, Subcommand};
use html_parser::Dom;
use serde::Deserialize;
use std::sync::LazyLock;
use std::{
    fmt::Display,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    process::Command,
};

/// Dioxus: build web, desktop, and mobile apps with a single codebase.
#[derive(Parser)]
#[clap(name = "dioxus", version = VERSION.as_str())]
#[clap(styles = CARGO_STYLING)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) action: Commands,

    #[command(flatten)]
    pub(crate) verbosity: Verbosity,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Create a new Dioxus project.
    #[clap(name = "new")]
    New(create::Create),

    /// Build, watch, and serve the project.
    #[clap(name = "serve")]
    Serve(serve::ServeArgs),

    /// Bundle the Dioxus app into a shippable object.
    #[clap(name = "bundle")]
    Bundle(bundle::Bundle),

    /// Build the Dioxus project and all of its assets.
    #[clap(name = "build")]
    Build(CommandWithPlatformOverrides<build::BuildArgs>),

    /// Run the project without any hotreloading.
    #[clap(name = "run")]
    Run(run::RunArgs),

    /// Init a new project for Dioxus in the current directory (by default).
    /// Will attempt to keep your project in a good state.
    #[clap(name = "init")]
    Init(init::Init),

    /// Diagnose installed tools and system configuration.
    #[clap(name = "doctor")]
    Doctor(doctor::Doctor),

    /// Translate a source file into Dioxus code.
    #[clap(name = "translate")]
    Translate(translate::Translate),

    /// Automatically format RSX.
    #[clap(name = "fmt")]
    Autoformat(autoformat::Autoformat),

    /// Check the project for any issues.
    #[clap(name = "check")]
    Check(check::Check),

    /// Dioxus config file controls.
    #[clap(subcommand)]
    #[clap(name = "config")]
    Config(config::Config),

    /// Update the Dioxus CLI to the latest version.
    #[clap(name = "self-update")]
    SelfUpdate(update::SelfUpdate),

    /// Run a dioxus build tool. IE `build-assets`, etc
    #[clap(name = "tools")]
    #[clap(subcommand)]
    Tools(BuildTools),
}

#[derive(Subcommand)]
pub enum BuildTools {
    /// Build the assets for a specific target.
    #[clap(name = "assets")]
    BuildAssets(build_assets::BuildAssets),
}

impl Display for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Commands::Build(_) => write!(f, "build"),
            Commands::Translate(_) => write!(f, "translate"),
            Commands::Serve(_) => write!(f, "serve"),
            Commands::New(_) => write!(f, "create"),
            Commands::Init(_) => write!(f, "init"),
            Commands::Config(_) => write!(f, "config"),
            Commands::Autoformat(_) => write!(f, "fmt"),
            Commands::Check(_) => write!(f, "check"),
            Commands::Bundle(_) => write!(f, "bundle"),
            Commands::Run(_) => write!(f, "run"),
            Commands::SelfUpdate(_) => write!(f, "self-update"),
            Commands::Tools(_) => write!(f, "tools"),
            Commands::Doctor(_) => write!(f, "doctor"),
        }
    }
}

pub(crate) static VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{} ({})",
        crate::dx_build_info::PKG_VERSION,
        crate::dx_build_info::GIT_COMMIT_HASH_SHORT.unwrap_or("was built without git repository")
    )
});

/// Cargo's color style
/// [source](https://github.com/crate-ci/clap-cargo/blob/master/src/style.rs)
pub(crate) const CARGO_STYLING: Styles = Styles::styled()
    .header(styles::HEADER)
    .usage(styles::USAGE)
    .literal(styles::LITERAL)
    .placeholder(styles::PLACEHOLDER)
    .error(styles::ERROR)
    .valid(styles::VALID)
    .invalid(styles::INVALID);

pub mod styles {
    use super::*;
    pub(crate) const HEADER: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
    pub(crate) const USAGE: Style = AnsiColor::Green.on_default().effects(Effects::BOLD);
    pub(crate) const LITERAL: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
    pub(crate) const PLACEHOLDER: Style = AnsiColor::Cyan.on_default();
    pub(crate) const ERROR: Style = AnsiColor::Red.on_default().effects(Effects::BOLD);

    pub(crate) const VALID: Style = AnsiColor::Cyan.on_default().effects(Effects::BOLD);
    pub(crate) const INVALID: Style = AnsiColor::Yellow.on_default().effects(Effects::BOLD);

    // extra styles for styling logs
    // we can style stuff using the ansi sequences like: "hotpatched in {GLOW_STYLE}{}{GLOW_STYLE:#}ms"
    pub(crate) const GLOW_STYLE: Style = AnsiColor::Yellow.on_default();
    pub(crate) const NOTE_STYLE: Style = AnsiColor::Green.on_default();
    pub(crate) const LINK_STYLE: Style = AnsiColor::Blue.on_default();
    pub(crate) const ERROR_STYLE: Style = AnsiColor::Red.on_default();
    pub(crate) const HINT_STYLE: Style = clap::builder::styling::Ansi256Color(244).on_default();
}
