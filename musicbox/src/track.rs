use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    path: PathBuf,
    title: String,
}

impl Track {
    pub fn new(path: &Path) -> Track {
        let title = match path.file_stem() {
            Some(name) => name.to_string_lossy().to_string(),
            None => path.display().to_string(),
        };

        Track {
            path: path.to_owned(),
            title,
        }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.path.display().fmt(f)
    }
}
