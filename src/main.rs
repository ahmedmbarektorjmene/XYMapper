mod app;
mod config;
mod controllers;
mod error;
mod mapping;
mod ui;
mod xbox;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ui::wiring::run()?;
    Ok(())
}
