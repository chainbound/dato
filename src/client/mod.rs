mod api;

#[allow(clippy::module_inception)]
mod client;
pub use client::Client;

mod spec;
pub use spec::ClientSpec;
