use tracing_subscriber::layer::SubscriberExt;

pub fn init_tracing(default_level: &str) -> anyhow::Result<()> {
    let format = std::env::var("NEUROHID_LOG_FORMAT")
        .unwrap_or_else(|_| "json".to_string())
        .to_ascii_lowercase();

    let directive: tracing_subscriber::filter::Directive =
        format!("neurohid={default_level}").parse()?;

    let make_filter = || {
        tracing_subscriber::EnvFilter::from_default_env()
            .add_directive(directive.clone())
    };

    match format.as_str() {
        "text" | "pretty" => tracing::subscriber::set_global_default(
            tracing_subscriber::registry()
                .with(make_filter())
                .with(tracing_subscriber::fmt::layer()),
        )
        .map_err(|error| anyhow::anyhow!("Failed to initialize tracing: {}", error)),
        _ => tracing::subscriber::set_global_default(
            tracing_subscriber::registry().with(make_filter()).with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_ansi(false)
                    .with_current_span(true)
                    .with_span_list(true),
            ),
        )
        .map_err(|error| anyhow::anyhow!("Failed to initialize tracing: {}", error)),
    }
}
