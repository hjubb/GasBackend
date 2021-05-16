#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate rocket;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cached::proc_macro::cached;
use clokwerk::{Scheduler, TimeUnits};
use load_dotenv::load_dotenv;
use reqwest::blocking::get;
use rocket::{Config, State};
use rocket::config::Environment;
use rocket_contrib::json::Json;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

load_dotenv!();

#[derive(Serialize)]
struct GasResponse {
    average: u16,
    recent: Vec<u16>,
}

#[derive(Deserialize)]
struct GasRequest {
    average: u16,
}

const URL: &'static str = concat!("https://data-api.defipulse.com/api/v1/egs/api/ethgasAPI.json?api-key=", env!("DEFI_DATA_KEY"));

#[cached(size = 1, time = 60, option = true)]
fn get_gas() -> Option<u16> {
    let resp = get(URL);
    if let Ok(res) = resp {
        let body = res.json::<GasRequest>();
        if let Ok(obj) = body {
            Some(obj.average)
        } else {
            None
        }
    } else {
        None
    }
}

#[get("/gas_price")]
fn index(cache: State<Arc<Mutex<VecDeque<u16>>>>) -> Option<Json<GasResponse>> {
    if let Some(current_gas) = get_gas() {
        let current = cache.lock().unwrap();
        let cloned = Vec::from(current.clone());
        Some(Json(GasResponse { average: current_gas, recent: cloned }))
    } else {
        None
    }
}

#[tokio::main]
async fn main() {
    let config = Config::build(Environment::Production)
        .address("0.0.0.0")
        .port(2048)
        .unwrap();

    let mut cache: VecDeque<u16> = VecDeque::with_capacity(50);
    for _ in 0..50 {
        cache.push_front(0);
    }
    let arc = Arc::new(Mutex::new(cache));

    let arc_ref = arc.clone();

    let mut scheduler = Scheduler::new();
    scheduler.every(5.minutes()).run(move || {
        let resp = get(URL);
        if !resp.is_ok() {
            return;
        }
        let body = resp.unwrap().json::<GasRequest>();
        if !body.is_ok() {
            return;
        }
        let cache_mutex = &arc_ref;
        let mut cache = cache_mutex.lock().unwrap();
        cache.push_front(body.unwrap().average);
        while cache.len() > 50 {
            cache.pop_back();
        }
        return;
    });

    let thread_handle = scheduler.watch_thread(Duration::from_secs(60));

    rocket::custom(config)
        .manage(arc.clone())
        .mount("/", routes![index])
        .launch();

    thread_handle.stop();
}
