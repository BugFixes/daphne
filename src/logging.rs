use std::sync::OnceLock;

static LOGGER: OnceLock<bugfixes::BugfixesLogger> = OnceLock::new();

pub fn init_global() {
    let _ = bugfixes::init_global(logger().clone());
    bugfixes::install_global_panic_hook();
}

#[track_caller]
pub fn info(message: impl Into<String>) {
    let _ = logger().info(message.into());
}

#[track_caller]
pub fn warn(message: impl Into<String>) {
    let _ = logger().warn(message.into());
}

fn logger() -> &'static bugfixes::BugfixesLogger {
    LOGGER.get_or_init(|| {
        bugfixes::BugfixesLogger::new(logger_config())
            .expect("Failed to initialize Bugfixes logger")
    })
}

fn logger_config() -> bugfixes::Config {
    let mut config = bugfixes::Config::from_env();

    // Default to local-only output until remote credentials are configured.
    if config.agent_key.is_empty() || config.agent_secret.is_empty() {
        config.local_only = true;
    }

    config
}

#[cfg(test)]
mod tests {
    use super::logger_config;

    #[test]
    fn defaults_to_local_only_without_bugfixes_credentials() {
        temp_env::with_vars(
            [
                ("BUGFIXES_AGENT_KEY", None),
                ("BUGFIXES_AGENT_SECRET", None),
                ("BUGFIXES_LOCAL_ONLY", Some("false")),
            ],
            || {
                let config = logger_config();

                assert!(config.local_only);
            },
        );
    }

    #[test]
    fn preserves_remote_logging_when_bugfixes_credentials_exist() {
        temp_env::with_vars(
            [
                ("BUGFIXES_AGENT_KEY", Some("agent-key")),
                ("BUGFIXES_AGENT_SECRET", Some("agent-secret")),
                ("BUGFIXES_LOCAL_ONLY", Some("false")),
                ("BUGFIXES_LOG_LEVEL", Some("warn")),
                ("BUGFIXES_SERVER", Some("https://bugfixes.example/v1")),
            ],
            || {
                let config = logger_config();

                assert!(!config.local_only);
                assert_eq!(config.agent_key, "agent-key");
                assert_eq!(config.agent_secret, "agent-secret");
                assert_eq!(config.log_level, "warn");
                assert_eq!(config.server, "https://bugfixes.example/v1");
            },
        );
    }
}
