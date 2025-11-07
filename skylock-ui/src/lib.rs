use std::path::Path;
use skylock_core::{Result, SkylockError};

#[derive(Debug, Clone, Copy)]
pub enum DeletionChoice {
    DeleteEverywhere,
    DeleteLocalOnly,
    Cancel,
}

// Windows implementation using native-windows-gui
#[cfg(windows)]
mod windows_impl {
    use super::*;
    use native_windows_gui as nwg;

    fn init_gui() -> Result<()> {
        nwg::init().map_err(|_| SkylockError::System(SystemErrorType::ProcessFailed))
    }

    pub fn show_deletion_prompt(file_path: &Path) -> Result<DeletionChoice> {
        init_gui()?;

        let params = nwg::MessageParams {
            title: &format!("Delete '{}'", file_path.display()),
            content: &format!(
                "Do you want to delete '{}' from Hetzner Storage Box as well?",
                file_path.display()
            ),
            buttons: nwg::MessageButtons::YesNoCancel,
            icons: nwg::MessageIcons::Question,
        };
        let choice = nwg::message(&params);

        match choice {
            nwg::MessageChoice::Yes => Ok(DeletionChoice::DeleteEverywhere),
            nwg::MessageChoice::No => Ok(DeletionChoice::DeleteLocalOnly),
            nwg::MessageChoice::Cancel => Ok(DeletionChoice::Cancel),
            _ => Ok(DeletionChoice::Cancel), // Handle all other cases as Cancel
        }
    }

    pub fn show_error(title: &str, message: &str) -> Result<()> {
        init_gui()?;
        nwg::simple_message(title, message);
        Ok(())
    }

    pub fn show_progress(title: &str, message: &str) -> Result<()> {
        init_gui()?;
        nwg::simple_message(title, message);
        Ok(())
    }
}

// Unix implementation using console prompts
#[cfg(unix)]
mod unix_impl {
    use super::*;
    use std::io::{self, Write};

    pub fn show_deletion_prompt(file_path: &Path) -> Result<DeletionChoice> {
        print!(
            "Delete '{}'?\n[y] Delete everywhere [n] Delete locally [c] Cancel: ",
            file_path.display()
        );
        io::stdout().flush().map_err(|e| SkylockError::Other(e.to_string()))?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| SkylockError::Other(e.to_string()))?;
        
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => Ok(DeletionChoice::DeleteEverywhere),
            "n" | "no" => Ok(DeletionChoice::DeleteLocalOnly),
            _ => Ok(DeletionChoice::Cancel),
        }
    }

    pub fn show_error(title: &str, message: &str) -> Result<()> {
        eprintln!("Error: {}: {}", title, message);
        Ok(())
    }

    pub fn show_progress(title: &str, message: &str) -> Result<()> {
        println!("{}: {}", title, message);
        Ok(())
    }
}

// Public API that delegates to platform-specific implementations
pub fn show_deletion_prompt(file_path: &Path) -> Result<DeletionChoice> {
    #[cfg(windows)]
    return windows_impl::show_deletion_prompt(file_path);
    
    #[cfg(unix)]
    return unix_impl::show_deletion_prompt(file_path);
    
    #[cfg(not(any(windows, unix)))]
    {
        eprintln!("Warning: GUI not supported on this platform, defaulting to cancel");
        Ok(DeletionChoice::Cancel)
    }
}

pub fn show_error(title: &str, message: &str) -> Result<()> {
    #[cfg(windows)]
    return windows_impl::show_error(title, message);
    
    #[cfg(unix)]
    return unix_impl::show_error(title, message);
    
    #[cfg(not(any(windows, unix)))]
    {
        eprintln!("{}: {}", title, message);
        Ok(())
    }
}

pub fn show_progress(title: &str, message: &str) -> Result<()> {
    #[cfg(windows)]
    return windows_impl::show_progress(title, message);
    
    #[cfg(unix)]
    return unix_impl::show_progress(title, message);
    
    #[cfg(not(any(windows, unix)))]
    {
        println!("{}: {}", title, message);
        Ok(())
    }
}
