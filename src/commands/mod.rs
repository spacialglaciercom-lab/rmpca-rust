//! Command implementations for rmpca CLI

pub mod bundle;
pub mod clean;
pub mod compile_map;
pub mod extract_overture;
pub mod extract_osm;
pub mod logs;
pub mod optimize;
pub mod pipeline;
pub mod route;
pub mod serve;
pub mod status;
pub mod validate;

// Export argument structs for main.rs
pub use bundle::BundleArgs;
pub use clean::CleanArgs;
pub use compile_map::CompileMapArgs;
pub use extract_overture::ExtractOvertureArgs;
pub use extract_osm::ExtractOsmArgs;
pub use logs::LogsArgs;
pub use optimize::OptimizeArgs;
pub use pipeline::PipelineArgs;
pub use route::RouteArgs;
pub use serve::ServeArgs;
pub use status::StatusArgs;
pub use validate::ValidateArgs;
