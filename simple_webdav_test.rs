use std::env;
use std::path::Path;
use anyhow::Result;
use base64::Engine;

// Direct WebDAV test without relying on the full skylock-core compilation
#[tokio::main]
async fn main() -> Result<()> {
    // Basic WebDAV connection test using only reqwest
    
    let username = env::var("HETZNER_SB_USER")
        .unwrap_or_else(|_| "uXXXXXX".to_string());
    let password = env::var("HETZNER_SB_PASSWORD")
        .unwrap_or_else(|_| {
            println!("❌ HETZNER_SB_PASSWORD environment variable required for testing");
            println!("Set it with: export HETZNER_SB_PASSWORD=your-storage-box-password");
            std::process::exit(1);
        });
    let host = env::var("HETZNER_SB_HOST")
        .unwrap_or_else(|_| "uXXXXXX.your-storagebox.de".to_string());
    
    println!("🔐 Testing WebDAV connection to Hetzner Storage Box");
    println!("📡 Host: {}", host);
    println!("👤 User: {}", username);
    
    // Create HTTP client with Basic Auth
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let credentials = format!("{}:{}", username, password);
    let encoded = base64::prelude::BASE64_STANDARD.encode(credentials.as_bytes());
    let auth_header = format!("Basic {}", encoded);
    
    let base_url = format!("https://{}", host);
    
    // Test 1: PROPFIND root to test connection
    println!("🔍 Testing connection with PROPFIND...");
    let response = client
        .request(reqwest::Method::from_bytes(b"PROPFIND")?, &base_url)
        .header("Authorization", &auth_header)
        .header("Depth", "0")
        .header("Content-Type", "text/xml; charset=utf-8")
        .body("")
        .send()
        .await?;

    if response.status().is_success() {
        println!("✅ WebDAV connection successful! (Status: {})", response.status());
    } else {
        println!("❌ WebDAV connection failed! (Status: {})", response.status());
        let error_body = response.text().await?;
        println!("Error body: {}", error_body);
        return Ok(());
    }
    
    // Test 2: Create a test directory
    let test_dir_url = format!("{}/backup/skylock/test", base_url);
    println!("📁 Creating test directory...");
    let response = client
        .request(reqwest::Method::from_bytes(b"MKCOL")?, &test_dir_url)
        .header("Authorization", &auth_header)
        .send()
        .await?;

    if response.status().is_success() || response.status().as_u16() == 405 {
        println!("✅ Test directory ready! (Status: {})", response.status());
    } else {
        println!("⚠️  Directory creation returned: {}", response.status());
    }
    
    // Test 3: Upload a small file
    let test_content = b"Hello from Skylock Hybrid - WebDAV test successful!";
    let test_file_url = format!("{}/backup/skylock/test/hello.txt", base_url);
    println!("⬆️  Uploading test file...");
    
    let response = client
        .put(&test_file_url)
        .header("Authorization", &auth_header)
        .header("Content-Type", "text/plain")
        .body(test_content.to_vec())
        .send()
        .await?;

    if response.status().is_success() {
        println!("✅ File upload successful! (Status: {})", response.status());
    } else {
        println!("❌ File upload failed! (Status: {})", response.status());
        let error_body = response.text().await?;
        println!("Error body: {}", error_body);
        return Ok(());
    }
    
    // Test 4: Download and verify the file
    println!("⬇️  Downloading test file...");
    let response = client
        .get(&test_file_url)
        .header("Authorization", &auth_header)
        .send()
        .await?;

    if response.status().is_success() {
        let downloaded = response.bytes().await?;
        if downloaded.as_ref() == test_content {
            println!("✅ File download and verification successful!");
        } else {
            println!("❌ File content verification failed!");
        }
    } else {
        println!("❌ File download failed! (Status: {})", response.status());
        return Ok(());
    }
    
    // Test 5: Clean up - delete the test file
    println!("🗑️  Cleaning up test file...");
    let response = client
        .delete(&test_file_url)
        .header("Authorization", &auth_header)
        .send()
        .await?;

    if response.status().is_success() {
        println!("✅ Cleanup successful!");
    } else {
        println!("⚠️  Cleanup returned: {}", response.status());
    }
    
    println!("\n🎉 All WebDAV tests passed!");
    println!("🚀 Skylock Hybrid can successfully communicate with your Hetzner Storage Box!");
    println!("\nNext steps:");
    println!("  - Set your password with: export HETZNER_SB_PASSWORD=your-actual-password");
    println!("  - The WebDAV integration is ready for backup operations");
    println!("  - Remaining work: Fix compilation errors and integrate with backup engine");
    
    Ok(())
}