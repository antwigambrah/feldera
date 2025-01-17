use std::sync::Arc;

use clap::{Args, Command, FromArgMatches};

use colored::Colorize;

use pipeline_manager::config::{DatabaseConfig, LocalRunnerConfig};
use pipeline_manager::db::ProjectDB;
use pipeline_manager::local_runner;
use tokio::spawn;
use tokio::sync::Mutex;

// Entrypoint to bring up the standalone compiler service.
#[tokio::main]
async fn main() {
    let name = "[local-runner]".cyan();
    pipeline_manager::logging::init_logging(name);
    let cli = Command::new("Feldera local runner service");
    let cli = DatabaseConfig::augment_args(cli);
    let cli = LocalRunnerConfig::augment_args(cli);
    let matches = cli.get_matches();

    let database_config = DatabaseConfig::from_arg_matches(&matches)
        .map_err(|err| err.exit())
        .unwrap();
    let local_runner_config = LocalRunnerConfig::from_arg_matches(&matches)
        .map_err(|err| err.exit())
        .unwrap();
    let local_runner_config = local_runner_config.canonicalize().unwrap();
    let db = ProjectDB::connect(
        &database_config,
        #[cfg(feature = "pg-embed")]
        None,
    )
    .await
    .unwrap();
    let db = Arc::new(Mutex::new(db));
    let _local_runner = spawn(async move {
        local_runner::run(db, &local_runner_config.clone()).await;
    });
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c event");
}
