use std::iter::FromIterator;
use std::ops::Deref;
use std::str::FromStr;

use anyhow::Result;
use git2::{Config, ErrorClass, ErrorCode};
use log::*;

type GitResult<T> = std::result::Result<T, git2::Error>;

#[derive(Debug)]
pub enum ConfigValue<T> {
    Explicit { value: T, source: String },
    Implicit(T),
}

impl<T> ConfigValue<T> {
    pub fn unwrap(self) -> T {
        match self {
            ConfigValue::Explicit { value: x, .. } | ConfigValue::Implicit(x) => x,
        }
    }

    pub fn is_implicit(&self) -> bool {
        match self {
            ConfigValue::Explicit { .. } => false,
            ConfigValue::Implicit(_) => true,
        }
    }
}

impl<T> Deref for ConfigValue<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            ConfigValue::Explicit { value: x, .. } | ConfigValue::Implicit(x) => x,
        }
    }
}

pub struct ConfigBuilder<'a, T> {
    config: &'a Config,
    key: &'a str,
    explicit: Option<(&'a str, T)>,
    default: Option<&'a T>,
}

pub fn get<'a, T>(config: &'a Config, key: &'a str) -> ConfigBuilder<'a, T> {
    ConfigBuilder {
        config,
        key,
        explicit: None,
        default: None,
    }
}

impl<'a, T> ConfigBuilder<'a, T>
where
    T: Clone,
{
    pub fn with_explicit(self, source: &'a str, value: Option<T>) -> ConfigBuilder<'a, T> {
        if let Some(value) = value {
            ConfigBuilder {
                explicit: Some((source, value)),
                ..self
            }
        } else {
            self
        }
    }

    pub fn with_default(self, value: &'a T) -> ConfigBuilder<'a, T> {
        ConfigBuilder {
            default: Some(value),
            ..self
        }
    }
}

impl<'a, T> ConfigBuilder<'a, T>
where
    T: ConfigValues + Clone,
{
    pub fn read(self) -> GitResult<Option<ConfigValue<T>>> {
        if let Some((source, value)) = self.explicit {
            return Ok(Some(ConfigValue::Explicit {
                value,
                source: source.to_string(),
            }));
        }
        match T::get_config_value(self.config, self.key) {
            Ok(value) => Ok(Some(ConfigValue::Explicit {
                value,
                source: self.key.to_string(),
            })),
            Err(err) if config_not_exist(&err) => {
                if let Some(default) = self.default {
                    Ok(Some(ConfigValue::Implicit(default.clone())))
                } else {
                    Ok(None)
                }
            }
            Err(err) => Err(err),
        }
    }
}

impl<'a, T> ConfigBuilder<'a, T>
where
    T: Clone,
{
    pub fn parse_with<F>(self, parse: F) -> Result<Option<ConfigValue<T>>>
    where
        F: FnOnce(&str) -> Result<T>,
    {
        if let Some((source, value)) = self.explicit {
            return Ok(Some(ConfigValue::Explicit {
                value,
                source: source.to_string(),
            }));
        }

        let result = match self.config.get_str(self.key) {
            Ok(value) => Some(ConfigValue::Explicit {
                value: parse(value)?,
                source: self.key.to_string(),
            }),
            Err(err) if config_not_exist(&err) => {
                if let Some(default) = self.default {
                    Some(ConfigValue::Implicit(default.clone()))
                } else {
                    None
                }
            }
            Err(err) => return Err(err.into()),
        };
        Ok(result)
    }

    pub fn parse_multi_with<F>(self, parse: F) -> Result<Option<ConfigValue<T>>>
    where
        F: FnOnce(&[String]) -> Result<T>,
    {
        if let Some((source, value)) = self.explicit {
            return Ok(Some(ConfigValue::Explicit {
                value,
                source: source.to_string(),
            }));
        }

        let result = match Vec::<String>::get_config_value(self.config, self.key) {
            Ok(values) if !values.is_empty() => Some(ConfigValue::Explicit {
                value: parse(&values)?,
                source: self.key.to_string(),
            }),
            Ok(_) => {
                if let Some(default) = self.default {
                    Some(ConfigValue::Implicit(default.clone()))
                } else {
                    None
                }
            }
            Err(err) => return Err(err.into()),
        };
        Ok(result)
    }
}

impl<'a, T> ConfigBuilder<'a, T> {
    pub fn parse(self) -> Result<Option<ConfigValue<T>>>
    where
        T: FromStr + Clone,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        self.parse_with(|str| Ok(str.parse()?))
    }

    pub fn parse_flatten<U>(self) -> Result<Option<ConfigValue<T>>>
    where
        T: FromStr + IntoIterator<Item = U> + FromIterator<U> + Clone,
        T::Err: std::error::Error + Send + Sync + 'static,
    {
        self.parse_multi_with(|strings| {
            let mut result = Vec::new();
            for x in strings {
                result.push(T::from_str(x)?.into_iter())
            }
            Ok(T::from_iter(result.into_iter().flatten()))
        })
    }
}

pub trait ConfigValues {
    fn get_config_value(config: &Config, key: &str) -> Result<Self, git2::Error>
    where
        Self: Sized;
}

impl ConfigValues for String {
    fn get_config_value(config: &Config, key: &str) -> Result<Self, git2::Error> {
        config.get_string(key)
    }
}

impl ConfigValues for Vec<String> {
    fn get_config_value(config: &Config, key: &str) -> Result<Self, git2::Error> {
        let mut result = Vec::new();
        for entry in &config.entries(Some(key))? {
            let entry = entry?;
            if let Some(value) = entry.value() {
                result.push(value.to_string());
            } else {
                warn!(
                    "non utf-8 config entry {}",
                    String::from_utf8_lossy(entry.name_bytes())
                );
            }
        }
        Ok(result)
    }
}

impl ConfigValues for bool {
    fn get_config_value(config: &Config, key: &str) -> Result<Self, git2::Error> {
        config.get_bool(key)
    }
}

fn config_not_exist(err: &git2::Error) -> bool {
    err.code() == ErrorCode::NotFound && err.class() == ErrorClass::Config
}

pub fn get_push_remote(config: &Config, branch: &str) -> Result<ConfigValue<String>> {
    if let Some(push_remote) = get(config, &format!("branch.{}.pushRemote", branch))
        .parse_with(|push_remote| Ok(push_remote.to_string()))?
    {
        return Ok(push_remote);
    }

    if let Some(push_default) =
        get(config, "remote.pushDefault").parse_with(|push_default| Ok(push_default.to_string()))?
    {
        return Ok(push_default);
    }

    get_remote(config, branch)
}

pub fn get_remote(config: &Config, branch: &str) -> Result<ConfigValue<String>> {
    Ok(get(config, &format!("branch.{}.remote", branch))
        .with_default(&String::from("origin"))
        .read()?
        .expect("has default"))
}
