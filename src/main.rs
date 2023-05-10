use hyper::{
    header::CONTENT_TYPE,
    service::{make_service_fn, service_fn},
    Body, Request, Response, Server,
};
use lazy_static::lazy_static;
use prometheus::{labels, opts, register_counter, register_gauge, register_histogram_vec};
use prometheus::{Counter, Encoder, Gauge, HistogramVec, TextEncoder};
use serde::Deserialize;
use std::fs;
use std::iter::Map;
use toml::Table;

lazy_static! {
    static ref HTTP_COUNTER: Counter = register_counter!(opts!(
        "example_http_requests_total",
        "Number of HTTP requests made.",
        labels! {"handler" => "all",}
    ))
    .unwrap();
    static ref HTTP_BODY_GAUGE: Gauge = register_gauge!(opts!(
        "example_http_response_size_bytes",
        "The HTTP response sizes in bytes.",
        labels! {"handler" => "all",}
    ))
    .unwrap();
    static ref HTTP_REQ_HISTOGRAM: HistogramVec = register_histogram_vec!(
        "example_http_request_duration_seconds",
        "The HTTP request latencies in seconds.",
        &["handler"]
    )
    .unwrap();
}

async fn serve_req(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    let encoder = TextEncoder::new();

    HTTP_COUNTER.inc();
    let timer = HTTP_REQ_HISTOGRAM.with_label_values(&["all"]).start_timer();

    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    HTTP_BODY_GAUGE.set(buffer.len() as f64);

    let response = Response::builder()
        .status(200)
        .header(CONTENT_TYPE, encoder.format_type())
        .body(Body::from(buffer))
        .unwrap();

    timer.observe_duration();

    Ok(response)
}

#[derive(Deserialize)]
struct Config {
    title: String,
    databases: Vec<Database>
}

#[derive(Deserialize)]
struct Database {
    driver: String,
    hostname: String,
    port: u16,
    username: String,
    password: String,
    database: String,
    metrics: Vec<Metric>
}

#[derive(Deserialize)]
struct Metric {
    name: String,
    frequency: String,
}

fn parse_config() ->  Config {
    let config = fs::read_to_string("example.toml").expect("Config not found");
    toml::from_str(&config).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const test_config: &str = r###"
title = "Default Jikji Config"

[[databases]]
driver = "postgres"
hostname = "127.0.0.1"
port = 5432
username = "postgres"
password= "secret"
database= "postgres"

[[databases.metrics]]
name="hubspot.actions.delayed"
type="counter"
frequency="15m"
query = """ \
    select count(*) from actions_scheduled
    where completed is null
    and scheduled < now() - interval '15 minutes'
    and scheduled > now() - interval '1 day';
    """
"###;

    fn config() ->  Config {
        toml::from_str(test_config).unwrap()
    }

    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }

    #[test]
    fn parses_name() {
        assert_eq!(
            config().title,
            String::from("Default Jikji Config")
        );

    }

    #[test]
    fn parses_database_driver() {
        assert_eq!(
            config().databases.get(0).unwrap().driver,
            String::from("postgres")
        );
    }

    #[test]
    fn parses_metric_name() {
        assert_eq!(
            config().databases.get(0).unwrap().metrics.get(0).unwrap().name,
            String::from("hubspot.actions.delayed")
        );
    }
}


#[tokio::main]
async fn main() {
    let config = parse_config();
    let addr = ([127, 0, 0, 1], 9898).into();
    println!("Listening on http://{}", addr);

    let serve_future = Server::bind(&addr).serve(make_service_fn(|_| async {
        Ok::<_, hyper::Error>(service_fn(serve_req))
    }));

    if let Err(err) = serve_future.await {
        eprintln!("server error: {}", err);
    }
}
