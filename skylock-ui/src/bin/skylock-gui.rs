//! Skylock GUI Application Binary
//!
//! Run the graphical user interface for Skylock backup management.

#[cfg(feature = "gui")]
fn main() {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Run the GUI
    if let Err(e) = skylock_ui::run_gui() {
        eprintln!("Error running GUI: {}", e);
        std::process::exit(1);
    }
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("GUI feature not enabled. Rebuild with --features gui");
    std::process::exit(1);
}
