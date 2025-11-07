pub mod error_types;
pub mod handler;

pub use error_types::*;
pub use handler::*;

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
