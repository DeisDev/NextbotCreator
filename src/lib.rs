pub mod catalog;
pub mod converter;
pub mod domain;
pub mod generator;
pub mod integration;
pub mod persistence;

pub const APP_NAME: &str = "NextbotCreator";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_FILE: &str = "nextbotcreator.json";
