use std::borrow::Borrow;
use std::fmt::Display;

use log::error;

pub type MusicResult<T> = Result<T, String>;
pub type VoidResult = MusicResult<()>;

pub trait ErrorExt<G, E> {
    fn as_err(self) -> MusicResult<G>;

    fn format<F: FnOnce(E) -> String>(self, f: F) -> MusicResult<G>;

    fn prefix<P: Borrow<str>>(self, prefix: P) -> MusicResult<G>;

    fn format_log<F: FnOnce(E) -> String>(self, f: F) -> MusicResult<G>;

    fn log(self) -> Self;

    fn drop(self);
}

impl<G, E> ErrorExt<G, E> for Result<G, E>
where
    E: Display,
{
    fn as_err(self) -> Result<G, String> {
        self.map_err(|e| e.to_string())
    }

    fn format<F>(self, f: F) -> MusicResult<G>
    where
        F: FnOnce(E) -> String,
    {
        self.map_err(|e| f(e))
    }

    fn prefix<P>(self, prefix: P) -> MusicResult<G>
    where
        P: Borrow<str>,
    {
        self.map_err(|e| format!("{}: {}", prefix.borrow(), e))
    }

    fn format_log<F>(self, f: F) -> MusicResult<G>
    where
        F: FnOnce(E) -> String,
    {
        self.format(f).log()
    }

    fn log(self) -> Self {
        self.map_err(|e| {
            error!("{}", e);
            e
        })
    }

    fn drop(self) {}
}
