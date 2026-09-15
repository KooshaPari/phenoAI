use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use image::{ImageBuffer, Rgba};
use reqwest::Client;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const ONE_BY_ONE_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

async fn fetch_screenshot(
    base_url: &str,
    timeout: Duration,
) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let client = Client::builder()
        .timeout(timeout)
        .build()
        .context("building screenshot API client")?;
    let response = client
        .get(format!("{base_url}/screenshot"))
        .send()
        .await
        .context("issuing screenshot request")?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_else(|err| format!("<failed to read body: {err}>"));
        return Err(anyhow!("screenshot request failed with {status}: {body}"));
    }

    let bytes = response.bytes().await.context("reading screenshot body")?;
    let image = image::load_from_memory(&bytes)
        .context("decoding screenshot body as image")?
        .into_rgba8();
    Ok(image)
}

#[tokio::test]
async fn decodes_successful_screenshot_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ONE_BY_ONE_PNG.to_vec(), "image/png"))
        .mount(&server)
        .await;

    let image = fetch_screenshot(&server.uri(), Duration::from_secs(1))
        .await
        .expect("1x1 PNG response should decode");

    assert!(
        !image.as_raw().is_empty(),
        "decoded image buffer should be non-empty"
    );
    assert_eq!(image.width(), 1);
    assert_eq!(image.height(), 1);
}

#[tokio::test]
async fn surfaces_server_error_response_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(ResponseTemplate::new(500).set_body_string("service unavailable"))
        .mount(&server)
        .await;

    let error = fetch_screenshot(&server.uri(), Duration::from_secs(1))
        .await
        .expect_err("500 response should surface an error");

    let message = error.to_string();
    assert!(message.contains("500"));
    assert!(message.contains("service unavailable"));
}

#[tokio::test]
async fn fires_per_request_timeout_on_slow_response() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/screenshot"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_raw(ONE_BY_ONE_PNG.to_vec(), "image/png"),
        )
        .mount(&server)
        .await;

    let error = fetch_screenshot(&server.uri(), Duration::from_millis(100))
        .await
        .expect_err("slow response should trip the per-request timeout");

    let timed_out = error.chain().any(|source| {
        source
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|reqwest_error| reqwest_error.is_timeout())
    });
    assert!(timed_out, "expected reqwest timeout error, got: {error:#}");
}
