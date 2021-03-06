#[cfg(not(test))]
mod cli;
mod db;
#[cfg(not(test))]
mod endpoints;
mod error;
mod types;

#[cfg(not(test))]
#[tokio::main]
async fn main() -> error::Result<()> {
    pretty_env_logger::init_timed();

    log::info!("Starting Efficio…");
    let opt: cli::Opt = argh::from_env();
    endpoints::routes::start_server(&opt).await
}
