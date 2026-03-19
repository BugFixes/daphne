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
