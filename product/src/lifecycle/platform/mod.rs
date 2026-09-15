//! OS preparation and protected local storage. Unsupported protection fails closed.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;
#[cfg(not(unix))]
mod windows;
#[cfg(not(unix))]
pub use windows::*;

/// Create only missing ancestors; never repair the access policy of existing state.
pub fn directory_tree(path: &std::path::Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        directory_tree(parent)?;
    }
    directory(path)
}
