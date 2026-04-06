use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = env::var_os("MEMOVYN_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".memovyn"));
        let database_path = env::var_os("MEMOVYN_DATABASE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("memovyn.sqlite3"));
        Self {
            data_dir,
            database_path,
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        if let Some(parent) = self.database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::create_dir_all(&self.data_dir)?;
        Ok(())
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }
}
