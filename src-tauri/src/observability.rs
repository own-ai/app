//! Observability setup for ownAI.
//!
//! Initializes tracing with optional Langfuse integration for LLM observability.
//! When Langfuse credentials are configured in the OS keychain, an OpenTelemetry
//! pipeline is set up that exports traces to Langfuse alongside the standard
//! fmt (console) layer. Without credentials, only console logging is active.

use crate::ai_instances::LangfuseKeyStorage;
use opentelemetry::trace::TracerProvider;
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
use opentelemetry_sdk::{resource::Resource, runtime::Tokio, trace::SdkTracerProvider};
use std::sync::OnceLock;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Dedicated Tokio runtime for OpenTelemetry's BatchSpanProcessor.
/// Lives for the entire app lifetime so the background export task keeps running.
static OTEL_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

/// Initialize the tracing subscriber.
///
/// If Langfuse credentials are found in the keychain, sets up an OpenTelemetry
/// tracing pipeline that exports spans to Langfuse, layered with console output.
/// Otherwise, falls back to plain console logging via `tracing_subscriber::fmt`.
///
/// Log level defaults to INFO (showing INFO, WARN, ERROR). Override with the
/// `RUST_LOG` environment variable, e.g. `RUST_LOG=debug` for more detail.
pub fn init_tracing() {
    // Check if Langfuse is configured
    let public_key = LangfuseKeyStorage::load_public_key().ok().flatten();
    let secret_key = LangfuseKeyStorage::load_secret_key().ok().flatten();

    if let (Some(pk), Some(sk)) = (public_key, secret_key) {
        let host = LangfuseKeyStorage::load_host();

        match setup_langfuse_tracing(&pk, &sk, &host) {
            Ok(()) => {
                tracing::info!("Langfuse observability enabled (host: {})", host);
            }
            Err(e) => {
                // Fall back to fmt-only if Langfuse setup fails
                eprintln!("Failed to initialize Langfuse tracing, falling back to console: {e}");
                init_fmt_only();
            }
        }
    } else {
        // No Langfuse credentials -- console logging only
        init_fmt_only();
    }
}

/// Initialize console-only logging with EnvFilter (default: info).
fn init_fmt_only() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Set up the full OpenTelemetry + Langfuse tracing pipeline.
fn setup_langfuse_tracing(
    public_key: &str,
    secret_key: &str,
    host: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build the Langfuse exporter with programmatic configuration
    let exporter = opentelemetry_langfuse::ExporterBuilder::new()
        .with_host(host)
        .with_basic_auth(public_key, secret_key)
        .build()?;

    // Create a dedicated Tokio runtime for the BatchSpanProcessor.
    // The runtime lives in a static OnceLock so the background export task keeps running for the entire app lifetime.
    let rt = OTEL_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("otel-export")
            .enable_all()
            .build()
            .expect("Failed to create OpenTelemetry runtime")
    });
    let _guard = rt.enter();

    // Create a tracer provider with batch span processor (async via dedicated runtime)
    let provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_attributes([KeyValue::new("service.name", "ownai")])
                .build(),
        )
        .with_span_processor(BatchSpanProcessor::builder(exporter, Tokio).build())
        .build();

    // Set the global tracer provider so rig-core's tracing spans are captured
    opentelemetry::global::set_tracer_provider(provider.clone());

    // Create the OpenTelemetry tracing layer
    let otel_layer = tracing_opentelemetry::layer().with_tracer(provider.tracer("ownai"));

    // Per-layer filtering:
    // - OTel layer: No filter (all spans go to Langfuse)
    // - fmt layer: INFO+ only on console (overridable via RUST_LOG)
    let fmt_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(tracing_subscriber::fmt::layer().with_filter(fmt_filter))
        .init();

    Ok(())
}

/// Build a `LangfuseContext` with standard metadata for a given AI instance.
///
/// This context is intended to be attached to OpenTelemetry spans so that
/// Langfuse traces include session, environment, and system metadata.
///
/// Returns `None` if Langfuse is not configured.
pub fn langfuse_context(
    instance_id: &str,
    instance_name: &str,
) -> Option<opentelemetry_langfuse::LangfuseContext> {
    if instance_id.is_empty() {
        return None;
    }
    if !LangfuseKeyStorage::is_configured() {
        return None;
    }

    let environment = if cfg!(debug_assertions) {
        "development"
    } else {
        "production"
    };

    let metadata = serde_json::json!({
        "environment": environment,
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "instance_name": instance_name,
    });

    let ctx = opentelemetry_langfuse::LangfuseContext::new();
    ctx.set_session_id(instance_id)
        .set_trace_name(format!("ownai-{}", instance_name))
        .add_tags(vec![environment.to_string()])
        .set_metadata(metadata);

    Some(ctx)
}
