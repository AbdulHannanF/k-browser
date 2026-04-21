/// KitsuneEngine demo server — standalone binary entry point.
///
/// Can be run with `cargo run -p kitsune-cloud-mock` to test the mock server
/// independently of the main browser binary.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    kitsune_cloud_mock::start("127.0.0.1:7700").await
}
