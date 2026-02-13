use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_tracing(default_level: &str) -> anyhow::Result<()> {
    let format = std::env::var("NEUROHID_LOG_FORMAT")
        .unwrap_or_else(|_| "json".to_string())
        .to_ascii_lowercase();

    let filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(format!("neurohid={default_level}").parse()?);

    match format.as_str() {
        "text" | "pretty" => tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .try_init(),
        _ => tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_ansi(false)
                    .with_current_span(true)
                    .with_span_list(true),
            )
            .try_init(),
    }
    .map_err(|error| anyhow::anyhow!("Failed to initialize tracing: {}", error))
}
