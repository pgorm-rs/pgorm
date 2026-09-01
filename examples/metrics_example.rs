use pgorm::metric::{InstrumentedPool, LoggingMetrics, NoOpMetrics};
use tokio_postgres::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable tracing to see the metrics output
    tracing_subscriber::fmt::init();

    // Create a regular database pool
    let mut config = Config::new();
    config.host("localhost");
    config.user("postgres");
    config.dbname("test");

    let pool = pgorm::connect(config);

    // Wrap it with no-op metrics (zero cost)
    let _no_op_pool = InstrumentedPool::new(pool.clone(), NoOpMetrics);

    // Wrap it with logging metrics
    let instrumented_pool = InstrumentedPool::new(pool, LoggingMetrics);

    println!(
        "Created instrumented pool with tag: {}",
        instrumented_pool.tag()
    );

    // This would record connection acquisition metrics
    // let conn = instrumented_pool.get().await?;

    // This would record query execution metrics
    // let rows = conn.query_all("SELECT 1", &[]).await?;

    println!("Metrics example completed - check implementation in METRICS.md");

    Ok(())
}
