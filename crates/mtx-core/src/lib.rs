//! Core of `mtx`, the MennoTeX package manager: TeX Live package database
//! parsing, the file index used for on-demand installation, repository
//! access, and the installer.

pub mod binaries;
pub mod bootstrap;
pub mod config;
pub mod configfiles;
pub mod consent;
pub mod ctx;
pub mod db;
pub mod doctor;
pub mod ensure;
pub mod extract;
pub mod fontmaps;
pub mod fontnames;
pub mod github;
pub mod formats;
pub mod index;
pub mod install;
pub mod logview;
pub mod lsr;
pub mod prefetch;
pub mod repo;
pub mod root;
pub mod shims;
pub mod verify;
pub mod tlpdb;

#[cfg(test)]
mod local_repo_tests;
