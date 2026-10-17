mod api;
mod app;
mod config;
mod converter;
mod error;
mod http;
mod limits;
mod network;
mod pipeline;
mod policy;
mod rules;
mod scripts;
mod subscription;
mod tickets;
mod upstream;
mod user_scripts;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run().await
}
