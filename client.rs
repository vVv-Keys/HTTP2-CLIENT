use hyper::{Client, Uri, Request, Body};
use hyper_tls::HttpsConnector;
use tokio::time::timeout;
use std::time::Duration;
use tracing::{info, error, debug};
use tracing_subscriber;
use structopt::StructOpt;
use std::fs::File;
use std::io::Write;
use http::Method;
use serde::Deserialize;
use dotenv::dotenv;
use std::env;

#[derive(StructOpt, Debug)]
#[structopt(name = "http2_client")]
struct Opt {
    #[structopt(short, long, default_value = "https://example.com")]
    url: String,

    #[structopt(short, long, default_value = "10")]
    timeout: u64,

    #[structopt(short, long, default_value = "GET")]
    method: String,

    #[structopt(long)]
    headers: Vec<String>,

    #[structopt(long)]
    output: Option<String>,

    #[structopt(long)]
    body: Option<String>,

    #[structopt(short, long)]
    verbose: bool,

    #[structopt(short, long, default_value = "3")]
    retries: u8,

    #[structopt(long)]
    follow_redirects: bool,
}

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    let opt = Opt::from_args();

    if opt.verbose {
        tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
    }

    let https = HttpsConnector::new();
    let client = Client::builder()
        .http2_only(true)
        .build::<_, hyper::Body>(https);

    let uri = match opt.url.parse::<Uri>() {
        Ok(uri) => uri,
        Err(e) => {
            error!("Failed to parse URI: {}", e);
            return;
        }
    };

    let method = match opt.method.to_uppercase().as_str() {
        "GET" => Method::GET,
        "POST" => Method::POST,
        "PUT" => Method::PUT,
        "DELETE" => Method::DELETE,
        "HEAD" => Method::HEAD,
        "OPTIONS" => Method::OPTIONS,
        "PATCH" => Method::PATCH,
        _ => {
            error!("Unsupported HTTP method: {}", opt.method);
            return;
        }
    };

    let mut request_builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("User-Agent", "hyper-http2-client/0.1");

    for header in opt.headers.iter() {
        let parts: Vec<&str> = header.splitn(2, ':').collect();
        if parts.len() == 2 {
            request_builder = request_builder.header(parts[0].trim(), parts[1].trim());
        } else {
            error!("Invalid header format: {}", header);
            return;
        }
    }

    let body = if let Some(body_str) = &opt.body {
        Body::from(body_str.clone())
    } else {
        Body::empty()
    };

    let request = request_builder
        .body(body)
        .expect("Request builder");

    let timeout_duration = Duration::from_secs(opt.timeout);

    for attempt in 1..=opt.retries {
        match timeout(timeout_duration, client.request(request.clone())).await {
            Ok(Ok(response)) => {
                info!("Response Status: {}", response.status());
                for (key, value) in response.headers() {
                    info!("{}: {:?}", key, value);
                }

                let body = hyper::body::to_bytes(response.into_body()).await;
                match body {
                    Ok(bytes) => {
                        let body_str = String::from_utf8_lossy(&bytes);
                        info!("Response Body: {}", body_str);

                        if let Some(output) = &opt.output {
                            let mut file = File::create(output).expect("Unable to create file");
                            file.write_all(&bytes).expect("Unable to write data");
                        }
                    }
                    Err(e) => {
                        error!("Failed to read response body: {}", e);
                    }
                }
                break;
            }
            Ok(Err(e)) => {
                error!("Request failed: {} (Attempt {}/{})", e, attempt, opt.retries);
            }
            Err(_) => {
                error!("Request timed out (Attempt {}/{})", attempt, opt.retries);
            }
        }
    }
}
