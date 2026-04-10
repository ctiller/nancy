use axum::{
    http::{header::CONTENT_TYPE, StatusCode, Uri},
    response::IntoResponse,
    Router,
    routing::get,
};
use leptos_axum::{generate_route_list, LeptosRoutes};
use std::sync::atomic::Ordering;
use tower_http::trace::TraceLayer;
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

async fn fs_asset_handler(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {

    
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    if path.contains(".git/") || path.starts_with(".git") {
        return (StatusCode::FORBIDDEN, "Forbidden").into_response();
    }
    let target = match tokio::fs::canonicalize(root.join(&path)).await {
        Ok(t) => t,
        Err(_) => return (StatusCode::NOT_FOUND, "Not Found").into_response(),
    };
    
    let root_canon = tokio::fs::canonicalize(&root).await.unwrap_or(root);

    if !target.starts_with(&root_canon) {
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

async fn proxy_grinder_state(
    axum::extract::Path(did): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let socket_path = root.join(".nancy").join("sockets").join(&did).join("grinder.sock");

    if !socket_path.exists() {
        tracing::debug!("Proxy error: UDS socket not found at {:?}", socket_path);
        return (StatusCode::NO_CONTENT, "Grinder socket not found").into_response();
    }

    if let Ok(client) = reqwest::Client::builder().unix_socket(socket_path).build() {
        let url = if let Some(last_update) = params.get("last_update") {
            format!("http://localhost/live-state?last_update={}", last_update)
        } else {
            "http://localhost/live-state".to_string()
        };
        
        match client.get(&url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if let Ok(data) = resp.bytes().await {
                    if status != 200 {
                        tracing::debug!("Proxy error: grinder backend returned {} with body {:?}", status, data);
                        return (status, data).into_response();
                    }
                    return ([(CONTENT_TYPE, "application/json")], data).into_response();
                } else {
                    tracing::debug!("Proxy error: failed to read bytes from grinder backend");
                }
            }
            Err(e) => {
                tracing::debug!("Proxy error: client.get failed: {}", e);
            }
        }
    } else {
        tracing::debug!("Proxy error: failed to build reqwest client with unix socket");
    }

    (StatusCode::BAD_GATEWAY, "Failed to pull from grinder").into_response()
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
        .route("/api/grinders", get(|| async {
            let res = web::agents::get_active_grinders().await.unwrap_or_default();
            axum::Json(res)
        }))
        .route("/api/{*fn_name}", axum::routing::post(leptos_axum::handle_server_fns))
        .route("/api/fs/{*path}", get(fs_asset_handler))
        .route("/api/grinders/{did}/state", get(proxy_grinder_state))
        .leptos_routes_with_context(&leptos_options, routes, move || {
            leptos::prelude::provide_context(options_clone.clone());
        }, web::Shell)
        .fallback(static_asset_handler)
        .layer(TraceLayer::new_for_http())
        .with_state(leptos_options);

    tokio::spawn(async move {
        let shutdown_signal = async {
            if !crate::commands::coordinator::SHUTDOWN.load(Ordering::SeqCst) {
                crate::commands::coordinator::SHUTDOWN_NOTIFY.notified().await;
            }
        };
        axum::serve(tcp_listener, web_app).with_graceful_shutdown(shutdown_signal).await.ok();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Uri;

    #[tokio::test]
    async fn test_static_asset_handler() {
        let uri = Uri::builder().path_and_query("/nancy-avatar.png").build().unwrap();
        let resp = static_asset_handler(uri).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        
        let uri_404 = Uri::builder().path_and_query("/not_found_test_123.png").build().unwrap();
        let resp_404 = static_asset_handler(uri_404).await.into_response();
        assert_eq!(resp_404.status(), StatusCode::NOT_FOUND);
    }
    
    #[tokio::test]
    async fn test_fs_asset_handler() {
        // Test valid file within workspace boundaries
        let path = axum::extract::Path("Cargo.toml".to_string());
        let resp = fs_asset_handler(path).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        
        // Test out of bounds constraint
        let path_oob = axum::extract::Path("../../../../etc/passwd".to_string());
        let resp_oob = fs_asset_handler(path_oob).await.into_response();
        assert_eq!(resp_oob.status(), StatusCode::FORBIDDEN);
        
        // Test gitignore constraint (.git directory is implicitly ignored)
        let path_git = axum::extract::Path(".git/config".to_string());
        let resp_git = fs_asset_handler(path_git).await.into_response();
        assert_eq!(resp_git.status(), StatusCode::FORBIDDEN);
        
        // Test not found constraint
        let path_nf = axum::extract::Path("does_not_exist_ever_123.txt".to_string());
        let resp_nf = fs_asset_handler(path_nf).await.into_response();
        assert_eq!(resp_nf.status(), StatusCode::NOT_FOUND);
    }
}
