use axum::{
    routing::{get, post},
    Router,
};
use std::env;
use std::net::SocketAddr;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod api;
mod compiler;
mod sandbox;

const AUTHOR: &str = "smileyan";
const GITHUB_URL: &str = "https://github.com/smile-yan/rust-playground-backend";

fn print_version() {
    println!("{}", env!("CARGO_PKG_VERSION"));
}

fn print_about() {
    println!("Author: {}", AUTHOR);
    println!("GitHub: {}", GITHUB_URL);
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-v" => {
                print_version();
                return Ok(());
            }
            "--about" => {
                print_about();
                return Ok(());
            }
            _ => {}
        }
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .route("/evaluate.json", post(api::evaluate))
        .route("/api/run", post(api::evaluate))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], 9001));
    info!("Rust Playground server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
