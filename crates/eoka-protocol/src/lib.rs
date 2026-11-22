#[macro_use]
mod catalog_macro;

mod agent;
mod args;
mod catalog;
mod context;
mod datadome;
mod download;
pub mod launch_spec;
mod metadata;
mod network;
mod operations;
mod response;
mod results;
pub mod sidecar;
mod turnstile;
pub mod wire;

pub use agent::*;
pub use args::*;
pub use catalog::*;
pub use context::*;
pub use datadome::*;
pub use download::*;
pub use launch_spec::{auto_connect, resolve_cdp_spec, resolve_proxy_spec, LaunchSpec};
pub use metadata::{
    OperationCapability, OperationExposure, OperationNeeds, OperationReply, ToolManifestEntry,
};
pub use network::*;
pub use operations::*;
pub use response::{ErrorDetail, Response, ResponseMeta};
pub use results::*;
pub use turnstile::*;

#[cfg(test)]
mod tests;
