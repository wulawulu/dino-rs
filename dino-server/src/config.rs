use std::path::Path;

use anyhow::{Context, Result};
use axum::http::Method;
use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub routes: ProjectRoutes,
}

pub type ProjectRoutes = IndexMap<String, Vec<ProjectRoute>>;

#[derive(Debug, Deserialize)]
pub struct ProjectRoute {
    #[serde(deserialize_with = "deserialize_method")]
    pub method: Method,
    pub handler: String,
}

fn deserialize_method<'de, D>(deserializer: D) -> Result<Method, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    match s.to_uppercase().as_str() {
        "GET" => Ok(Method::GET),
        "POST" => Ok(Method::POST),
        "PUT" => Ok(Method::PUT),
        "DELETE" => Ok(Method::DELETE),
        "PATCH" => Ok(Method::PATCH),
        "HEAD" => Ok(Method::HEAD),
        "OPTIONS" => Ok(Method::OPTIONS),
        "CONNECT" => Ok(Method::CONNECT),
        "TRACE" => Ok(Method::TRACE),
        _ => Err(serde::de::Error::custom("Invalid method")),
    }
}

impl ProjectConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let config = std::fs::read_to_string(path).context("Failed to read config file")?;
        let config: ProjectConfig = serde_yaml::from_str(&config)?;
        Ok(config)
    }
}
