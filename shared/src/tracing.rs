use std::fmt;
use tracing::Level;
use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{
        FmtContext, FormatEvent, FormatFields,
        format::{self},
    },
    layer::SubscriberExt,
    registry::LookupSpan,
};

pub struct TracingConfig {
    pub level: Level,
    pub json_output: bool,
    pub enable_file_logging: bool,
    pub file_path: Option<String>,
    #[allow(dead_code)]
    pub enable_metrics: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            json_output: false,
            enable_file_logging: false,
            file_path: None,
            enable_metrics: false,
        }
    }
}

/// Custom formatter: prefix + plain white message
struct CustomFormatter;

impl<S, N> FormatEvent<S, N> for CustomFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let level = metadata.level();

        // Write colored level prefix with bold colon
        match *level {
            Level::ERROR => {
                write!(
                    writer,
                    "{}{} ",
                    nu_ansi_term::Color::Red.bold().paint("error"),
                    nu_ansi_term::Style::new().bold().paint(":")
                )?;
            }
            Level::WARN => {
                write!(
                    writer,
                    "{}{} ",
                    nu_ansi_term::Color::Yellow.bold().paint("warning"),
                    nu_ansi_term::Style::new().bold().paint(":")
                )?;
            }
            Level::INFO => {
                write!(
                    writer,
                    "{}{} ",
                    nu_ansi_term::Color::Green.bold().paint("info"),
                    nu_ansi_term::Style::new().bold().paint(":")
                )?;
            }
            Level::DEBUG => {
                write!(
                    writer,
                    "{}{} ",
                    nu_ansi_term::Color::Blue.bold().paint("debug"),
                    nu_ansi_term::Style::new().bold().paint(":")
                )?;
            }
            Level::TRACE => {
                write!(
                    writer,
                    "{}{} ",
                    nu_ansi_term::Color::Purple.bold().paint("trace"),
                    nu_ansi_term::Style::new().bold().paint(":")
                )?;
            }
        }

        // Write the message in plain white (no styling)
        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

pub fn init_tracing(config: TracingConfig) -> anyhow::Result<()> {
    // Environment filter (allows RUST_LOG env var)
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config.level.as_str()));

    let console_layer = if config.json_output {
        tracing_subscriber::fmt::layer()
            .json()
            .with_current_span(true)
            .with_target(true)
            .with_filter(env_filter.clone())
            .boxed()
    } else {
        // Use custom formatter for colored output
        tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_level(false)
            .with_thread_ids(false)
            .with_thread_names(false)
            .without_time()
            .event_format(CustomFormatter)
            .with_filter(env_filter.clone())
            .boxed()
    };

    let file_layer = if config.enable_file_logging {
        let file_path = config
            .file_path
            .unwrap_or_else(|| "thala-node.log".to_string());
        let file = std::fs::File::create(file_path)?;
        Some(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::sync::Arc::new(file))
                .with_filter(env_filter.clone()),
        )
    } else {
        None
    };

    // Build the subscriber with all layers
    let subscriber = Registry::default().with(console_layer).with(file_layer);

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}
