use std::path::PathBuf;

pub const DAEMON_HOST: &str = "127.0.0.1";
pub const DAEMON_PORT: u16 = 7345;

pub fn base_dir() -> PathBuf {
    let mut base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.push(".mcphub");
    base
}

pub fn state_path() -> PathBuf {
    let mut base = base_dir();
    base.push("state.json");
    base
}
