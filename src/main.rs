extern crate actix_cors;
extern crate actix_web;
extern crate biscuit;
extern crate chrono;
extern crate cis_client;
extern crate cis_profile;
extern crate config;
extern crate env_logger;
extern crate failure;
extern crate futures;
extern crate reqwest;
extern crate serde;
extern crate tokio;
extern crate url;

#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;

mod groups;
mod healthz;
mod settings;
mod update_app;
mod updater;

use crate::healthz::healthz_app;
use crate::update_app::update_app;
use crate::updater::InternalUpdater;
use crate::updater::Updater;
use crate::updater::UpdaterClient;
use actix_web::middleware::Logger;
use actix_web::web;
use actix_web::App;
use actix_web::HttpServer;
use cis_client::CisClient;
use std::thread::spawn;

fn main() -> Result<(), String> {
    ::std::env::set_var("RUST_LOG", "actix_web=info,dino_park_evo=info");
    env_logger::init();
    info!("fire up ice age");
    let s = settings::Settings::new().map_err(|e| format!("unable to load settings: {}", e))?;
    let cis_client = CisClient::from_settings(&s.cis)
        .map_err(|e| format!("unable to create cis_client: {}", e))?;
    // Start http server
    let updater = InternalUpdater::new(cis_client.clone());

    let client = updater.client();
    let stop_client = updater.client();
    let updater_thread = spawn(move || tokio::runtime::current_thread::run(updater.run()));

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default().exclude("/healthz"))
            .service(web::scope("/v2").service(update_app(client.clone())))
            .service(healthz_app())
    })
    .bind("0.0.0.0:8085")
    .map_err(|e| format!("failed starting the server: {}", e))?
    .run()
    .map_err(|e| format!("crashing: {}", e))?;
    info!("Stopped http server");
    stop_client.stop();
    updater_thread
        .join()
        .map_err(|_| String::from("failed to stop updater"))?;
    Ok(())
}
