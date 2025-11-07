use std::path::Path;
use tokio::fs;
use tracing::info;

async fn initialize_application() -> Result<()> {
    info!("Initializing Skylock application...");

    // Create necessary directories
    let dirs = [
        "data/secrets",
        "data/recovery",
        "data/cache",
        "data/logs",
        "data/backup",
        "data/config",
    ];

    for dir in dirs {
        let path = Path::new(dir);
        if !path.exists() {
            fs::create_dir_all(path).await?;
            info!("Created directory: {}", dir);
        }
    }

    // Initialize credential manager
    let credential_manager = CredentialManager::new(
        Path::new("data/secrets/master.key").to_path_buf()
    )?;

    // Create default configuration if it doesn't exist
    let config_path = Path::new("config.toml");
    if !config_path.exists() {
        fs::write(config_path, include_str!("../config.toml")).await?;
        info!("Created default configuration file");
    }

    info!("Initialization complete!");
    Ok(())
}

async fn store_credentials() -> Result<()> {
    info!("Setting up secure credentials...");

    // Initialize credential manager
    let credential_manager = CredentialManager::new(
        Path::new("data/secrets/master.key").to_path_buf()
    )?;

    // Read credentials from environment or prompt user
    let hetzner_key = std::env::var("SKYLOCK_HETZNER_KEY")
        .unwrap_or_else(|_| prompt_credential("Enter Hetzner API key: "));

    let syncthing_key = std::env::var("SKYLOCK_SYNCTHING_KEY")
        .unwrap_or_else(|_| prompt_credential("Enter Syncthing API key: "));

    // Store credentials securely
    credential_manager.store_credential("hetzner_api_key", &hetzner_key).await?;
    credential_manager.store_credential("syncthing_api_key", &syncthing_key).await?;

    info!("Credentials stored securely!");
    Ok(())
}

fn prompt_credential(prompt: &str) -> String {
    use std::io::{self, Write};

    print!("{}", prompt);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}
