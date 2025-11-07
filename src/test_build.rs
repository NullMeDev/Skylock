use skylock_core::Config;

fn main() {
    println!("Testing minimal build configuration...");
    match Config::load(Some("config.debug.toml".into())) {
        Ok(_) => println!("Config loaded successfully"),
        Err(e) => println!("Error loading config: {:?}", e),
    }
}
