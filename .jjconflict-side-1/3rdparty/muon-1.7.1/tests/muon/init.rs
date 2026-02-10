use tracing_subscriber::EnvFilter;

#[ctor::ctor]
fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_test_writer()
        .pretty()
        .init();
}
