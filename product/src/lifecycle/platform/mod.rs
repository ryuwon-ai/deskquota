//! OS preparation and protected local storage. Unsupported protection fails closed.
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;
#[cfg(not(unix))]
mod windows;
#[cfg(not(unix))]
pub use windows::*;
