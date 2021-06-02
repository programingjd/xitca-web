//! Multi-threaded server for Tcp/Udp/UnixDomain handling.

#![forbid(unsafe_code)]

mod builder;
mod server;
mod worker;

#[cfg(feature = "signal")]
mod signals;

pub mod net;

pub use builder::Builder;
pub use server::{ServerFuture, ServerHandle};