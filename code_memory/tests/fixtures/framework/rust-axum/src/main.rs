mod routes;

use axum::{routing::get, Router};
use routes::health;

fn main() {
    let _app = Router::new().route("/health", get(health));
}
