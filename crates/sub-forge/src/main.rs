mod app;
mod config;
mod converter;
mod error;
mod http;
mod limits;
mod links;
mod model;
mod network;
mod pipeline;
mod rules;
mod scripts;
mod service;
mod store;
mod subscription;
mod upstream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run().await
}
