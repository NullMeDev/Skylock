//! Build script for platform-specific linking
//! 
//! This script handles Windows-specific library linking requirements
//! and is a no-op on other platforms.

fn main() {
    // Only perform Windows-specific linking on Windows targets
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=build.rs");
        
        // Link against Windows system libraries needed for VSS (Volume Shadow Copy Service)
        println!("cargo:rustc-link-lib=ole32");
        println!("cargo:rustc-link-lib=oleaut32");
        println!("cargo:rustc-link-lib=uuid");
        
        // Link against VSS libraries if available
        // Note: These might not be available in all Windows SDK versions
        println!("cargo:rustc-link-lib=dylib=vssapi");
        println!("cargo:rustc-link-lib=dylib=VssApi");
        
        // Add Windows SDK library paths
        if let Ok(windows_kits_dir) = std::env::var("WindowsSdkDir") {
            if let Ok(target_arch) = std::env::var("CARGO_CFG_TARGET_ARCH") {
                let lib_arch = match target_arch.as_str() {
                    "x86_64" => "x64",
                    "x86" => "x86",
                    "aarch64" => "arm64",
                    _ => "x64", // default fallback
                };
                
                // Add Windows SDK lib path
                println!("cargo:rustc-link-search=native={}/lib/10.0.22621.0/um/{}", windows_kits_dir, lib_arch);
                println!("cargo:rustc-link-search=native={}/lib/winv6.3/um/{}", windows_kits_dir, lib_arch);
            }
        }
        
        // Set build-time environment variables for Windows
        println!("cargo:rustc-env=SKYLOCK_PLATFORM=windows");
        println!("cargo:rustc-env=SKYLOCK_VSS_ENABLED=1");
    }
    
    // Unix/Linux-specific setup
    #[cfg(unix)]
    {
        println!("cargo:rerun-if-changed=build.rs");
        
        // Set build-time environment variables for Unix
        println!("cargo:rustc-env=SKYLOCK_PLATFORM=unix");
        println!("cargo:rustc-env=SKYLOCK_VSS_ENABLED=0");
        
        // Check for LVM tools availability at build time
        if std::process::Command::new("which")
            .arg("lvcreate")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            println!("cargo:rustc-env=SKYLOCK_LVM_AVAILABLE=1");
        } else {
            println!("cargo:rustc-env=SKYLOCK_LVM_AVAILABLE=0");
        }
        
        // Check for ZFS tools availability
        if std::process::Command::new("which")
            .arg("zfs")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            println!("cargo:rustc-env=SKYLOCK_ZFS_AVAILABLE=1");
        } else {
            println!("cargo:rustc-env=SKYLOCK_ZFS_AVAILABLE=0");
        }
    }
    
    // Cross-platform build information
    println!("cargo:rustc-env=SKYLOCK_BUILD_TIME={}", 
             std::env::var("SOURCE_DATE_EPOCH")
                 .map(|epoch| epoch.parse::<i64>().unwrap_or(0))
                 .unwrap_or_else(|_| {
                     use std::time::{SystemTime, UNIX_EPOCH};
                     SystemTime::now()
                         .duration_since(UNIX_EPOCH)
                         .unwrap()
                         .as_secs() as i64
                 }));
    
    // Output build configuration
    println!("cargo:warning=Skylock build script completed for target: {}", 
             std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string()));
}
