use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub networks: Vec<NetworkSpec>,
    #[serde(default)]
    pub devices: Vec<DeviceSpec>,
}

impl Scenario {
    pub fn load(path: &Path) -> Result<Self, ScenarioError> {
        let source = fs::read_to_string(path).map_err(|source| ScenarioError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let scenario = serde_json::from_str(&source).map_err(|source| ScenarioError::Json {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(scenario)
    }
}

fn default_schema_version() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSpec {
    pub id: String,
    #[serde(default = "default_network_kind")]
    pub kind: String,
    #[serde(default)]
    pub cable_role: Option<String>,
    #[serde(default)]
    pub channels: BTreeMap<String, Scalar>,
}

fn default_network_kind() -> String {
    "cable".to_owned()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSpec {
    pub id: String,
    pub prefab: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub reference_id: Option<i32>,
    #[serde(default)]
    pub connections: BTreeMap<String, String>,
    #[serde(default)]
    pub fields: BTreeMap<String, Scalar>,
    #[serde(default)]
    pub slots: BTreeMap<String, BTreeMap<String, Scalar>>,
    #[serde(default)]
    pub memory: BTreeMap<String, Scalar>,
    #[serde(default)]
    pub ic: Option<IcSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IcSpec {
    pub program: PathBuf,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default)]
    pub registers: BTreeMap<String, Scalar>,
    #[serde(default)]
    pub stack: BTreeMap<String, Scalar>,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum Scalar {
    Number(f64),
    Text(String),
}

impl Scalar {
    pub fn as_f64(&self) -> Result<f64, String> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Text(value) => match value.as_str() {
                "NaN" | "nan" => Ok(f64::NAN),
                "Infinity" | "+Infinity" | "inf" | "+inf" => Ok(f64::INFINITY),
                "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
                "-0" => Ok(-0.0),
                _ => value
                    .parse::<f64>()
                    .map_err(|_| format!("`{value}` is not an IC10 number")),
            },
        }
    }
}

#[derive(Debug)]
pub enum ScenarioError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Json { path, source } => {
                write!(formatter, "invalid scenario {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ScenarioError {}
