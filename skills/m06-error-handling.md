# Error Handling

## Guidelines

- Use Result<T, E>
- Avoid unwrap() in library code
- Add context via map_err

## Example

fn read_config() -> Result<Config, ConfigError> {
let content = std::fs::read_to_string("config.toml")
.map_err(ConfigError::Io)?;

    toml::from_str(&content)
        .map_err(ConfigError::Parse)
}