mod app;
mod config;
mod controllers;
mod error;
mod mapping;
mod ui;
mod xbox;

use std::error::Error;

use slint::ComponentHandle;

fn main() -> Result<(), Box<dyn Error>> {
    let app = app::state::AppState::new()?;
    eprintln!("configuration directory: {}", app.config_dir().display());

    let window = ui::MainWindow::new()?;
    window.run()?;
    Ok(())
}
