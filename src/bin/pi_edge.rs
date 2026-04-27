use clap::Parser;
use qnmd_sol::edge::config::EdgeConfig;
use qnmd_sol::edge::service::run_edge_service;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let config = EdgeConfig::parse();
    run_edge_service(config)
}
