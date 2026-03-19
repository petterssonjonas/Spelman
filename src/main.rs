mod app;
mod audio;
mod config;
mod coordinator;
mod library;
mod playlist;
mod pomodoro;
mod ui;
mod util;

use std::path::PathBuf;

use clap::Parser;

use app::App;
use config::settings::Settings;

#[derive(Parser)]
#[command(name = "spelman", about = "A terminal music player", version)]
struct Cli {
    /// Audio file to play.
    file: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    // Initialize logging to file (don't pollute TUI).
    let log_dir = directories::ProjectDirs::from("", "", "spelman")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    std::fs::create_dir_all(&log_dir)?;

    let log_file = std::fs::File::create(log_dir.join("spelman.log"))?;
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();
    let settings = Settings::load();
    let mut app = App::new(settings);

    if let Some(ref file) = cli.file {
        let path = std::fs::canonicalize(file)?;
        app.play_file(path);
    }

    app.run()
}
