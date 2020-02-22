use rust_embed::RustEmbed;

#[cfg(not(feature = "rpi"))]
#[derive(RustEmbed)]
#[folder = "config/default"]
pub struct Config;

#[cfg(feature = "rpi")]
#[derive(RustEmbed)]
#[folder = "config/rpi"]
pub struct Config;

#[derive(RustEmbed)]
#[folder = "target/webapp"]
pub struct Webapp;
