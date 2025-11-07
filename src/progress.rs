use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;
use colored::*;

pub struct ProgressReporter {
    multi: MultiProgress,
}

impl ProgressReporter {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
        }
    }

    pub fn create_spinner(&self, message: &str) -> ProgressBar {
        let spinner = self.multi.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        spinner.set_message(message.to_string());
        spinner.enable_steady_tick(Duration::from_millis(100));
        spinner
    }

    pub fn create_progress_bar(&self, total: u64, message: &str) -> ProgressBar {
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({percent}%) {eta}")
                .unwrap()
                .progress_chars("=>-"),
        );
        pb.set_message(message.to_string());
        pb
    }

    pub fn finish_with_message(&self, pb: &ProgressBar, message: &str) {
        pb.finish_with_message(message.to_string());
    }
}

pub struct ErrorHandler;

impl ErrorHandler {
    pub fn print_error(title: &str, error: &str) {
        eprintln!("{} {}", "❌".red(), title.red().bold());
        eprintln!("   {}", error);
    }

    pub fn print_warning(title: &str, warning: &str) {
        eprintln!("{} {}", "⚠️".yellow(), title.yellow().bold());
        eprintln!("   {}", warning);
    }

    pub fn print_success(title: &str, message: &str) {
        println!("{} {}", "✅".green(), title.green().bold());
        println!("   {}", message);
    }

    pub fn print_info(title: &str, message: &str) {
        println!("{} {}", "ℹ️".blue(), title.blue().bold());
        println!("   {}", message);
    }

    pub fn suggest_solution(suggestion: &str) {
        println!("{} {}", "💡".bright_yellow(), "Suggestion:".bright_yellow().bold());
        println!("   {}", suggestion);
    }

    pub fn print_detailed_error(error: &anyhow::Error) {
        eprintln!("{} {}", "❌".red(), "Error Details:".red().bold());
        
        let mut current_error: &dyn std::error::Error = error.as_ref();
        let mut level = 0;
        
        loop {
            let indent = "   ".repeat(level + 1);
            if level == 0 {
                eprintln!("{}• {}", indent, current_error);
            } else {
                eprintln!("{}└─ Caused by: {}", indent, current_error);
            }
            
            match current_error.source() {
                Some(source) => {
                    current_error = source;
                    level += 1;
                    if level > 5 { // Prevent infinite loops
                        eprintln!("{}└─ ...", "   ".repeat(level + 1));
                        break;
                    }
                }
                None => break,
            }
        }
    }

    pub fn format_file_size(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    pub fn format_duration(duration: std::time::Duration) -> String {
        let secs = duration.as_secs();
        if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
        }
    }
}