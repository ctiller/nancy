use axum::{
    http::{header::CONTENT_TYPE, StatusCode, Uri},
    response::IntoResponse,
    Router,
};
use leptos_axum::{generate_route_list, LeptosRoutes};
use std::sync::atomic::Ordering;
use web::App;

#[derive(rust_embed::RustEmbed, Clone)]
#[folder = "target/site/"]
struct WebAssets;

async fn static_asset_handler(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/').to_string();
    match WebAssets::get(path.as_str()) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

pub fn spawn_web_server(tcp_listener: tokio::net::TcpListener) -> tokio::task::JoinHandle<()> {
    let conf = leptos::prelude::get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let web_app = Router::new()
        .leptos_routes(&leptos_options, routes, App)
        .fallback(static_asset_handler)
        .with_state(leptos_options);

    tokio::spawn(async move {
        let shutdown_signal = async {
            loop {
                if crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        };
        axum::serve(tcp_listener, web_app).with_graceful_shutdown(shutdown_signal).await.ok();
    })
}
