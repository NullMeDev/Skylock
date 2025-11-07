#!/bin/bash
# Quick test to check what's in the backup directories

cat > /tmp/test_list.rs << 'EOF'
use skylock_core::Config;
use skylock_hetzner::{HetznerClient, HetznerConfig};

#[tokio::main]
async fn main() {
    let config = Config::load(None).expect("Failed to load config");
    
    let hetzner_config = HetznerConfig {
        endpoint: config.hetzner.endpoint.clone(),
        username: config.hetzner.username.clone(),
        password: config.hetzner.password.clone(),
        api_token: config.hetzner.encryption_key.clone(),
        encryption_key: config.hetzner.encryption_key.clone(),
    };
    
    let client = HetznerClient::new(hetzner_config).expect("Failed to create client");
    
    println!("Listing /skylock/backups:");
    match client.list_files("/skylock/backups").await {
        Ok(files) => {
            for file in files {
                println!("  - {:?}", file.path);
            }
        }
        Err(e) => println!("Error: {:?}", e),
    }
}
EOF

rustc --edition 2021 /tmp/test_list.rs -o /tmp/test_list \
  --extern skylock_core=./target/release/deps/libskylock_core.rlib \
  --extern skylock_hetzner=./target/release/deps/libskylock_hetzner.rlib \
  --extern tokio=./target/release/deps/libtokio.rlib \
  -L ./target/release/deps

/tmp/test_list
