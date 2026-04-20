---
name: m06-error-handling
description: Error handling conventions for rfgui library code — Result<T, E> propagation, avoiding unwrap, and attaching context via map_err. Use whenever the user writes fallible functions, sees unwrap in library code, designs an error enum, or asks how to convert / wrap underlying errors (std::io, toml, etc.) in this project.
---

# Error Handling

## Guidelines

- Use Result<T, E>
- Avoid unwrap() in library code
- Add context via map_err

## Example

```rust
fn read_config() -> Result<Config, ConfigError> {
    let content = std::fs::read_to_string("config.toml")
        .map_err(ConfigError::Io)?;

    toml::from_str(&content)
        .map_err(ConfigError::Parse)
}
```
