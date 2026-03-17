//! Skillet: a skill discovery toolkit for AI agents.
//!
//! This library provides the core functionality for loading, searching,
//! and serving skills from repos as MCP prompts. The binary crate
//! adds the CLI (clap) and MCP server (tower-mcp) on top.

pub mod bm25;
pub mod cache;
pub mod config;
pub mod error;
pub mod git;
pub mod index;
pub mod project;
pub mod prompts;
pub mod repo;
pub mod scaffold;
pub mod search;
pub mod state;
pub mod suggest;

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;
