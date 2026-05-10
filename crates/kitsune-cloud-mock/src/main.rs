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

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("0.0.0.0:{}", port);
    
    kitsune_cloud_mock::start(&addr).await
}
