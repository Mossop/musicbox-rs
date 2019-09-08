use std::env::current_dir;
use std::path::PathBuf;
use std::process::exit;

use clap::{load_yaml, App};

use musicbox::MusicBox;

fn main() {
    env_logger::init();

    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let data_dir: PathBuf = match matches.value_of("data") {
        Some(path) => {
            let dir: PathBuf = match path.parse() {
                Ok(p) => p,
                Err(e) => {
                    println!("'{}' is an invalid path: {}", path, e);
                    exit(1);
                }
            };

            if dir.is_absolute() {
                dir
            } else {
                let mut current = match current_dir() {
                    Ok(d) => d,
                    Err(e) => {
                        println!("Current working directory is invalid: {}", e);
                        exit(1);
                    }
                };
                current.push(dir);
                current
            }
        }
        None => {
            let current = match current_dir() {
                Ok(d) => d,
                Err(e) => {
                    println!("Current working directory is invalid: {}", e);
                    exit(1);
                }
            };
            current.to_owned()
        }
    };

    let result = if matches.is_present("daemonize") {
        MusicBox::daemonize(&data_dir)
    } else {
        MusicBox::block(&data_dir)
    };

    if let Err(e) = result {
        println!("{}", e);
        exit(1);
    }
}
