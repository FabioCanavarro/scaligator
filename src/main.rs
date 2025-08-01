mod config;
mod controller;
mod scaler;
mod metrics;
mod alerts;

use actix_web::{get, App, HttpServer, HttpResponse, Responder};
use config::AppConfig;
use controller::run_controller;
use kube::{Client, Config as KubeConfig};
use tracing::{error, info};
use tracing_subscriber::{fmt, EnvFilter};
use anyhow::Context;
use tokio::signal;

#[get("/health")]
async fn health() -> impl Responder {
    HttpResponse::Ok().body("OK")
}

#[get("/ready")]
async fn ready() -> impl Responder {
    HttpResponse::Ok().body("READY")
}

fn init_logger() {

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));
    
    fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_line_number(true)
        .with_target(true)
        .compact()
        .init();
}

#[tokio::main]
async fn main() {
    println!("🚀 Starting scaligator...");
    init_logger();
    info!("Logger initialized");

    if let Err(e) = run().await {
        error!("❌ Scaligator failed: {:#}", e);
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    // dotenvy::dotenv().context("Something went wrong during loading dotenv")?;

    info!("🔧 Loading application config...");
    let app_config = AppConfig::from_env().context("Failed to load config")?;
    info!("✅ Config loaded: {:?}", app_config);

    info!("🔧 Inferring Kubernetes config...");
    let kube_config = KubeConfig::infer().await.context("Failed to infer kube config")?;
    let kube_client = Client::try_from(kube_config).context("Failed to create kube client")?;
    info!("✅ Kubernetes client initialized");

    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    let bind_addr = format!("0.0.0.0:{}", port);

    info!("🌐 Starting HTTP server on {}", bind_addr);
    let server = HttpServer::new(|| {
        App::new()
            .service(health)
            .service(ready)
            .service(alerts::handle_alerts)
    })
    .bind(&bind_addr)?
    .run();

    tokio::select! {
        res = server => {
            res.context("HTTP server crashed")?;
        }
        res = run_controller(kube_client, app_config) => {
            res.context("Controller crashed")?;
        }
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal. Exiting...");
        }
    }

    Ok(())
}
