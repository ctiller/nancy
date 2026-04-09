use axum::{
    http::{header::CONTENT_TYPE, StatusCode, Uri},
    response::IntoResponse,
    Router,
    routing::get,
};
use leptos_axum::{generate_route_list, LeptosRoutes};
use std::sync::atomic::Ordering;
use tower_http::trace::TraceLayer;
use web::{App, Shell};

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

async fn fs_asset_handler(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    use std::path::PathBuf;
    
    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let target = root.join(&path);

    if !target.starts_with(&root) {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }

    // Checking gitignore efficiently
    let mut is_ignored = true;
    for result in ignore::WalkBuilder::new(&target).max_depth(Some(0)).build() {
        if result.is_ok() {
            is_ignored = false;
            break;
        }
    }

    if is_ignored {
        return (StatusCode::FORBIDDEN, "Forbidden by gitignore").into_response();
    }

    match tokio::fs::read(&target).await {
        Ok(data) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(CONTENT_TYPE, mime.as_ref())], data).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

pub fn spawn_web_server(tcp_listener: tokio::net::TcpListener) -> tokio::task::JoinHandle<()> {
    let conf = leptos::prelude::get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let options_clone = leptos_options.clone();
    let routes = generate_route_list(move || {
        leptos::prelude::provide_context(options_clone.clone());
        App()
    });

    let options_clone = leptos_options.clone();
    let web_app = Router::new()
        .route("/api/{*fn_name}", axum::routing::post(leptos_axum::handle_server_fns))
        .route("/api/fs/{*path}", get(fs_asset_handler))
        .leptos_routes_with_context(&leptos_options, routes, move || {
            leptos::prelude::provide_context(options_clone.clone());
        }, web::Shell)
        .fallback(static_asset_handler)
        .layer(TraceLayer::new_for_http())
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
