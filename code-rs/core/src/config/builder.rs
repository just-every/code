use crate::config_loader::LoaderOverrides;
use std::path::PathBuf;
use toml::Value as TomlValue;

use super::sources;
use super::validation::{apply_toml_override, deserialize_config_toml_with_cli_warnings};
use super::{Config, ConfigOverrides, ConfigToml};

#[derive(Default, Debug, Clone)]
pub struct ConfigBuilder {
    cli_overrides: Vec<(String, TomlValue)>,
    overrides: ConfigOverrides,
    code_home: Option<PathBuf>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cli_overrides(mut self, cli_overrides: Vec<(String, TomlValue)>) -> Self {
        self.cli_overrides = cli_overrides;
        self
    }

    pub fn with_overrides(mut self, overrides: ConfigOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    pub fn with_code_home(mut self, code_home: PathBuf) -> Self {
        self.code_home = Some(code_home);
        self
    }

    pub fn load(self) -> std::io::Result<Config> {
        let code_home = match self.code_home {
            Some(path) => path,
            None => sources::find_code_home()?,
        };

        let mut root_value = sources::load_config_as_toml(&code_home)?;
        let cli_paths: Vec<String> = self.cli_overrides.iter().map(|(path, _)| path.clone()).collect();
        for (path, value) in self.cli_overrides.into_iter() {
            apply_toml_override(&mut root_value, &path, value);
        }

        let cfg = deserialize_config_toml_with_cli_warnings(&root_value, &cli_paths)?;
        let mut config = Config::load_from_base_config_with_overrides(cfg, self.overrides, code_home)?;

        let requirements = crate::config_loader::load_config_requirements_blocking(
            &config.code_home,
            LoaderOverrides::default(),
        )?;

        let mut constrained_approval_policy = requirements.approval_policy;
        constrained_approval_policy
            .set(config.approval_policy)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidInput, err))?;
        config.approval_policy = constrained_approval_policy.value();

        Ok(config)
    }

    pub fn load_toml(self) -> std::io::Result<ConfigToml> {
        let code_home = match self.code_home {
            Some(path) => path,
            None => sources::find_code_home()?,
        };

        let mut root_value = sources::load_config_as_toml(&code_home)?;
        let cli_paths: Vec<String> = self.cli_overrides.iter().map(|(path, _)| path.clone()).collect();
        for (path, value) in self.cli_overrides.into_iter() {
            apply_toml_override(&mut root_value, &path, value);
        }

        deserialize_config_toml_with_cli_warnings(&root_value, &cli_paths)
    }
}
