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

async fn get_api_grinders(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let mut rx = state.tx_ready.subscribe();
    
    if let Some(target_version) = params.get("last_version").and_then(|v| v.parse::<u64>().ok()) {
        let current_state = *rx.borrow_and_update();
        if current_state == target_version {
            tokio::select! {
                _ = rx.changed() => {}
                _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => {}
            }
        }
    }

    let version = *rx.borrow();

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let mut statuses = vec![];
    let identity = { state.shared_identity.read().await.clone() };
    
    let repo = git2::Repository::discover(&root).ok();
    let appview = repo.map(|r| crate::coordinator::appview::AppView::hydrate(&r, &identity, None));
    
    if let crate::schema::identity_config::Identity::Coordinator { workers, .. } = &identity {
        for worker in workers {
            let (next_restart_at_unix, failures, log_ref) = appview.as_ref().and_then(|av| {
                av.agent_crashes.get(&worker.did).map(|c| (c.next_restart_at_unix, c.failures, Some(c.log_ref.clone())))
            }).unwrap_or((None, None, None));
            
            statuses.push(web::schema::GrinderStatus {
                did: worker.did.clone(),
                is_online: false,
                next_restart_at_unix,
                failures,
                log_ref,
            });
        }
    }

    let sockets_dir = root.join(".nancy").join("sockets");
    if let Ok(mut entries) = tokio::fs::read_dir(&sockets_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta = entry.metadata().await;
            if meta.map(|m| m.is_dir()).unwrap_or(false) {
                let did = entry.file_name().to_string_lossy().to_string();
                if did != "coordinator" && tokio::fs::metadata(entry.path().join("grinder.sock")).await.is_ok() {
                    if let Some(existing) = statuses.iter_mut().find(|s| s.did == did) {
                        existing.is_online = true;
                    } else {
                        let (next_restart_at_unix, failures, log_ref) = appview.as_ref().and_then(|av| {
                            av.agent_crashes.get(&did).map(|c| (c.next_restart_at_unix, c.failures, Some(c.log_ref.clone())))
                        }).unwrap_or((None, None, None));
                        
                        statuses.push(web::schema::GrinderStatus {
                            did: did.to_string(),
                            is_online: true,
                            next_restart_at_unix,
                            failures,
                            log_ref,
                        });
                    }
                }
            }
        }
    }

    axum::Json(web::schema::GrindersResponse { version, grinders: statuses })
}

async fn get_api_incident_log(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
    axum::extract::Path(log_ref): axum::extract::Path<String>,
) -> impl IntoResponse {
    let identity = { state.shared_identity.read().await.clone() };
    let did = identity.get_did_owner().did.clone();
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    let repo = match git2::Repository::discover(&root) {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "No repo found").into_response(),
    };
    
    let branch_name = format!("refs/heads/nancy/{}", did);
    let branch_commit = repo.find_reference(&branch_name).ok().and_then(|r| r.peel_to_commit().ok());
    
    if let Some(commit) = branch_commit {
        if let Ok(tree) = commit.tree() {
            if let Some(incidents_entry) = tree.get_name("incidents") {
                if let Ok(incidents_obj) = incidents_entry.to_object(&repo) {
                    if let Ok(incidents_tree) = incidents_obj.into_tree() {
                        if let Some(log_entry) = incidents_tree.get_name(&log_ref) {
                            if let Ok(blob) = log_entry.to_object(&repo).and_then(|obj| obj.into_blob().map_err(|_| git2::Error::from_str("not blob"))) {
                                if let Ok(s) = std::str::from_utf8(blob.content()) {
                                    let html = ansi_to_html::convert(s).unwrap_or_else(|_| s.to_string());
                                    return ([(CONTENT_TYPE, "text/plain")], html).into_response();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    (StatusCode::NOT_FOUND, "Log not found").into_response()
}

async fn get_api_tasks_topology(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let mut rx = state.tx_ready.subscribe();
    
    if let Some(target_version) = params.get("last_version").and_then(|v| v.parse::<u64>().ok()) {
        let current_state = *rx.borrow_and_update();
        if current_state == target_version {
            tokio::select! {
                _ = rx.changed() => {}
                _ = crate::commands::coordinator::SHUTDOWN_NOTIFY.notified() => {}
            }
        }
    }

    let version = *rx.borrow();

    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let repo = match git2::Repository::discover(&root) {
        Ok(r) => r,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "No repo found").into_response(),
    };
    
    let identity = { state.shared_identity.read().await.clone() };
    
    let appview = crate::coordinator::appview::AppView::hydrate(&repo, &identity, None);
    let mut topology = appview.get_topology();
    topology.version = version;

    axum::Json(topology).into_response()
}

pub fn spawn_web_server(tcp_listener: tokio::net::TcpListener, ipc_state: crate::coordinator::ipc::IpcState) -> tokio::task::JoinHandle<()> {
    let conf = leptos::prelude::get_configuration(None).unwrap();
    let leptos_options = conf.leptos_options;
    let options_clone = leptos_options.clone();
    let routes = generate_route_list(move || {
        leptos::prelude::provide_context(options_clone.clone());
        App()
    });

    let options_clone = leptos_options.clone();
    let web_app = Router::new()
        .route("/api/grinders", get(get_api_grinders))
        .route("/api/tasks/topology", get(get_api_tasks_topology))
        .route("/api/{*fn_name}", axum::routing::post(leptos_axum::handle_server_fns))
        .route("/api/fs/{*path}", get(fs_asset_handler))
        .route("/api/incidents/{log_ref}", get(get_api_incident_log))
        .route("/api/grinders/{did}/state", get(proxy_grinder_state))
        .route("/api/add-grinder", axum::routing::post(crate::coordinator::ipc::add_grinder_handler))
        .route("/api/remove-grinder", axum::routing::post(crate::coordinator::ipc::remove_grinder_handler))
        .leptos_routes_with_context(&leptos_options, routes, move || {
            leptos::prelude::provide_context(options_clone.clone());
        }, web::Shell)
        .fallback(static_asset_handler)
        .layer(TraceLayer::new_for_http())
        .layer(axum::Extension(ipc_state))
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
