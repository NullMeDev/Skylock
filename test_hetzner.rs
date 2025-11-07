use std::env;
use std::path::Path;
use skylock_hetzner::{HetznerWebDAVClient, WebDAVConfig};
use tokio::fs;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Read credentials from environment variables  
    let username = env::var("HETZNER_SB_USER")
        .unwrap_or_else(|_| "uXXXXXX".to_string());
    let password = env::var("HETZNER_SB_PASSWORD")
        .expect("HETZNER_SB_PASSWORD environment variable must be set");
    let host = env::var("HETZNER_SB_HOST")
        .unwrap_or_else(|_| "uXXXXXX.your-storagebox.de".to_string());
    
    println!("Testing Hetzner WebDAV connection...");
    println!("Host: {}", host);
    println!("Username: {}", username);
    
    // Create WebDAV config
    let config = WebDAVConfig {
        base_url: format!("https://{}", host),
        username,
        password,
        base_path: "/backup/skylock/test".to_string(),
    };

    // Create client
    let client = HetznerWebDAVClient::new(config)?;
    
    // Test connection
    println!("Testing connection...");
    client.test_connection().await?;
    println!("✅ Connection successful!");
    
    // Create test file
    let test_content = b"Hello from Skylock Hybrid backup system!";
    let test_file_path = Path::new("./test_file.txt");
    fs::write(test_file_path, test_content).await?;
    println!("Created test file: {}", test_file_path.display());
    
    // Upload test file
    println!("Uploading test file...");
    client.upload_file(test_file_path, "test_upload.txt").await?;
    println!("✅ Upload successful!");
    
    // List files
    println!("Listing files...");
    let files = client.list_files("").await?;
    println!("Files found: {:?}", files);
    
    // Download test file
    println!("Downloading test file...");
    let download_path = Path::new("./downloaded_test.txt");
    client.download_file("test_upload.txt", download_path).await?;
    println!("✅ Download successful!");
    
    // Verify content
    let downloaded_content = fs::read(download_path).await?;
    if downloaded_content == test_content {
        println!("✅ Content verification successful!");
    } else {
        println!("❌ Content verification failed!");
    }
    
    // Clean up test file
    client.delete_file("test_upload.txt").await?;
    println!("✅ File deleted successfully!");
    
    // Clean up local files
    let _ = fs::remove_file(test_file_path).await;
    let _ = fs::remove_file(download_path).await;
    
    println!("🎉 All tests passed! Hetzner WebDAV integration is working!");
    
    Ok(())
}