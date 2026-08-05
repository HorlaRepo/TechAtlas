#![forbid(unsafe_code)]

pub mod admin_auth;
pub mod admin_domains;
pub mod admin_operations;
pub mod app;
pub mod health;
pub mod metrics;
pub mod openapi;
pub mod public;

#[cfg(test)]
mod admin_domains_tests;
#[cfg(test)]
mod health_tests;
