// ≈ an Express middleware:  (req, res, next) => { ...; next(); ... }
// `next.run(req).await` is the equivalent of calling next().

use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

pub async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(req).await;

    println!(
        "{method} {path} -> {} ({:?})",
        response.status().as_u16(),
        start.elapsed()
    );
    response
}
