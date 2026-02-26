//! Skillet: a skill registry toolkit for AI agents.
//!
//! This library provides the core functionality for loading, searching,
//! validating, packing, and publishing skill registries. The binary crate
//! adds the CLI (clap) and MCP server (tower-mcp) on top.

pub mod bm25;
pub mod cache;
pub mod config;
pub mod discover;
pub mod error;
pub mod git;
pub mod index;
pub mod install;
pub mod integrity;
pub mod manifest;
pub mod pack;
pub mod publish;
pub mod registry;
pub mod safety;
pub mod scaffold;
pub mod search;
pub mod state;
pub mod trust;
pub mod validate;
pub mod version;
