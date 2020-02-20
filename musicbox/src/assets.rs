use rust_embed::RustEmbed;

#[cfg(not(feature = "rpi"))]
#[derive(RustEmbed)]
#[folder = "musicbox/config/default"]
pub struct Config;

#[cfg(feature = "rpi")]
#[derive(RustEmbed)]
#[folder = "musicbox/config/rpi"]
pub struct Config;
