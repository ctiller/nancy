use axum::{
    http::{header::CONTENT_TYPE, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use std::sync::atomic::Ordering;
use tower_http::trace::TraceLayer;

#[derive(rust_embed::RustEmbed, Clone)]
#[folder = "src/web/site/"]
struct WebAssets;

// Discarded const forces a compile-time fetch error if missing
#[cfg(not(debug_assertions))]
const _: &[u8] = include_bytes!("../web/site/index.html");

async fn static_asset_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() {
        path = "index.html".to_string();
    }

    match WebAssets::get(path.as_str()) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            // SPA fallback routing
            match WebAssets::get("index.html") {
                Some(content) => ([(CONTENT_TYPE, "text/html")], content.data).into_response(),
                None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
            }
        }
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
    let socket_path_grinder = root.join(".nancy").join("sockets").join(&did).join("grinder.sock");
    let socket_path_dreamer = root.join(".nancy").join("sockets").join(&did).join("dreamer.sock");
    let socket_path = if socket_path_grinder.exists() {
        socket_path_grinder
    } else {
        socket_path_dreamer
    };

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
    
    let appview = tokio::task::spawn_blocking({
        let root = root.clone();
        let identity = identity.clone();
        move || {
            let repo = git2::Repository::discover(&root).ok();
            repo.map(|r| crate::coordinator::appview::AppView::hydrate(&r, &identity, None))
        }
    }).await.unwrap_or(None);
    
    if let crate::schema::identity_config::Identity::Coordinator { workers, dreamer, .. } = &identity {
        let mut agents = vec![];
        for w in workers { agents.push((w, "grinder")); }
        agents.push((dreamer, "dreamer"));
        
        for (worker, agent_type) in agents {
            let (next_restart_at_unix, failures, log_ref) = appview.as_ref().and_then(|av| {
                av.agent_crashes.get(&worker.did).map(|c| (c.next_restart_at_unix, c.failures, Some(c.log_ref.clone())))
            }).unwrap_or((None, None, None));
            
            statuses.push(schema::GrinderStatus {
                did: worker.did.clone(),
                agent_type: agent_type.to_string(),
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
                if did != "coordinator" {
                    let is_grinder = tokio::fs::metadata(entry.path().join("grinder.sock")).await.is_ok();
                    let is_dreamer = tokio::fs::metadata(entry.path().join("dreamer.sock")).await.is_ok();
                    if is_grinder || is_dreamer {
                        if let Some(existing) = statuses.iter_mut().find(|s| s.did == did) {
                            existing.is_online = true;
                            existing.agent_type = if is_dreamer { "dreamer".to_string() } else { "grinder".to_string() };
                        } else {
                            let (next_restart_at_unix, failures, log_ref) = appview.as_ref().and_then(|av| {
                                av.agent_crashes.get(&did).map(|c| (c.next_restart_at_unix, c.failures, Some(c.log_ref.clone())))
                            }).unwrap_or((None, None, None));
                            
                            statuses.push(schema::GrinderStatus {
                                did: did.to_string(),
                                agent_type: if is_dreamer { "dreamer".to_string() } else { "grinder".to_string() },
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
    }

    axum::Json(schema::GrindersResponse { version, grinders: statuses })
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
    let identity = { state.shared_identity.read().await.clone() };
    
    let appview_opt = tokio::task::spawn_blocking({
        let root = root.clone();
        let identity = identity.clone();
        move || {
            let repo = match git2::Repository::discover(&root) {
                Ok(r) => r,
                Err(_) => return None,
            };
            Some(crate::coordinator::appview::AppView::hydrate(&repo, &identity, None))
        }
    }).await.unwrap_or(None);
    
    let mut topology = if let Some(av) = appview_opt {
        av.get_topology()
    } else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "No repo found").into_response();
    };
    topology.version = version;

    axum::Json(topology).into_response()
}

async fn get_api_tasks_evaluations(
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
    
    let current_version = *rx.borrow();
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let identity = { state.shared_identity.read().await.clone() };
    
    let appview_opt = tokio::task::spawn_blocking({
        let root = root.clone();
        let identity = identity.clone();
        move || {
            let repo = match git2::Repository::discover(&root) {
                Ok(r) => r,
                Err(_) => return None,
            };
            Some(crate::coordinator::appview::AppView::hydrate(&repo, &identity, None))
        }
    }).await.unwrap_or(None);
    
    let mut evals = Vec::new();
    if let Some(av) = appview_opt {
        for (_, payload) in av.task_evaluations {
            evals.push(schema::TaskEvaluation {
                id: payload.evaluated_event_id,
                event_type: payload.event_type,
                score: payload.score,
                timestamp: payload.timestamp,
            });
        }
    }
    
    evals.sort_by(|a, b| b.score.cmp(&a.score));
    
    axum::Json(serde_json::json!({
        "version": current_version,
        "evaluations": evals
    })).into_response()
}

async fn api_get_repo_tree(axum::extract::Query(_params): axum::extract::Query<std::collections::HashMap<String, String>>) -> impl IntoResponse {
    // let _branch = _params.get("branch").cloned().unwrap_or_else(|| "main".to_string());
    axum::Json(serde_json::json!([]))
}

async fn api_get_repo_branches() -> impl IntoResponse {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Ok(repo) = git2::Repository::discover(&root) {
        if let Ok(branches) = repo.branches(Some(git2::BranchType::Local)) {
            let names: Vec<String> = branches.filter_map(|b| b.ok()).filter_map(|(b, _)| b.name().ok().flatten().map(|s| s.to_string())).collect();
            return axum::Json(names).into_response();
        }
    }
    axum::Json(Vec::<String>::new()).into_response()
}

async fn api_read_file_text() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "content": "" }))
}

async fn api_submit_task(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
    axum::Json(payload): axum::Json<crate::schema::task::TaskRequestPayload>,
) -> impl IntoResponse {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match crate::commands::add_task::add_task(&root, Some(payload.description), None).await {
        Ok(_) => {
            state.tx_ready.send_modify(|v| *v += 1);
            axum::Json(serde_json::json!({ "accepted": true })).into_response()
        }
        Err(e) => {
            tracing::error!("Failed to submit task via UI: {:#}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

async fn api_human_pending(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
) -> impl IntoResponse {
    let identity = { state.shared_identity.read().await.clone() };
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    let appview_opt = tokio::task::spawn_blocking({
        let root = root.clone();
        let identity = identity.clone();
        move || {
            let repo = match git2::Repository::discover(&root) {
                Ok(r) => r,
                Err(_) => return None,
            };
            Some(crate::coordinator::appview::AppView::hydrate(&repo, &identity, None))
        }
    }).await.unwrap_or(None);
    
    if let Some(av) = appview_opt {
        let asks: Vec<_> = av.active_asks.values().cloned().collect();
        let plan_reviews: Vec<_> = av.active_plan_reviews.values().cloned().collect();
        axum::Json(serde_json::json!({
            "asks": asks,
            "plan_reviews": plan_reviews
        })).into_response()
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "No repo found").into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct HumanActionPayload {
    pub item_ref: String,
    pub text_response: Option<String>,
}

async fn api_human_action(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
    axum::Json(payload): axum::Json<HumanActionPayload>,
) -> impl IntoResponse {
    let identity = { state.shared_identity.read().await.clone() };
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    let res = tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::discover(&root).map_err(|e| e.to_string())?;
        let writer = crate::events::writer::Writer::new(&repo, identity).map_err(|e| e.to_string())?;
        
        if let Some(text) = payload.text_response {
            writer.log_event(crate::schema::registry::EventPayload::HumanResponse(
                crate::schema::task::ResponsePayload {
                    item_ref: payload.item_ref.clone(),
                    text_response: text,
                }
            )).map_err(|e| e.to_string())?;
        } else {
            writer.log_event(crate::schema::registry::EventPayload::Seen(
                crate::schema::task::SeenPayload {
                    item_ref: payload.item_ref.clone(),
                    timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                }
            )).map_err(|e| e.to_string())?;
        }
        writer.commit_batch().map_err(|e| e.to_string())
    }).await.unwrap_or_else(|e| Err(e.to_string()));

    match res {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response()
    }
}

async fn api_get_market_state(
    axum::extract::Extension(state): axum::extract::Extension<crate::coordinator::ipc::IpcState>,
) -> impl IntoResponse {
    let market_state = crate::coordinator::market::ArbitrationMarket::get_market_state(&state.token_market).await;
    axum::Json(market_state).into_response()
}

pub fn spawn_web_server(tcp_listener: tokio::net::TcpListener, ipc_state: crate::coordinator::ipc::IpcState) -> tokio::task::JoinHandle<()> {
    assert!(
        WebAssets::get("index.html").is_some(),
        "FATAL: Frontend WASM bundle index.html was not explicitly embedded in WebAssets. Did the frontend compile logic fail?"
    );

    let web_app = Router::new()
        .route("/api/grinders", get(get_api_grinders))
        .route("/api/tasks/topology", get(get_api_tasks_topology))
        .route("/api/tasks/evaluations", get(get_api_tasks_evaluations))
        .route("/api/repo/tree", get(api_get_repo_tree))
        .route("/api/repo/branches", get(api_get_repo_branches))
        .route("/api/repo/file", get(api_read_file_text))
        .route("/api/tasks", post(api_submit_task))
        .route("/api/fs/{*path}", get(fs_asset_handler))
        .route("/api/incidents/{log_ref}", get(get_api_incident_log))
        .route("/api/grinders/{did}/state", get(proxy_grinder_state))
        .route("/api/add-grinder", post(crate::coordinator::ipc::add_grinder_handler))
        .route("/api/remove-grinder", post(crate::coordinator::ipc::remove_grinder_handler))
        .route("/api/human/pending", get(api_human_pending))
        .route("/api/human/action", post(api_human_action))
        .route("/api/market/state", get(api_get_market_state))
        .fallback(static_asset_handler)
        .layer(TraceLayer::new_for_http())
        .layer(axum::Extension(ipc_state));

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
    use sealed_test::prelude::*;

    #[tokio::test]
    async fn test_static_asset_handler() {
        let uri = Uri::builder().path_and_query("/nancy-avatar.png").build().unwrap();
        let resp = static_asset_handler(uri).await.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
        
        let uri_404 = Uri::builder().path_and_query("/not_found_test_123.png").build().unwrap();
        let resp_404 = static_asset_handler(uri_404).await.into_response();
        assert_eq!(resp_404.status(), StatusCode::OK);
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

    #[tokio::test]
    async fn test_empty_api_endpoints() {
        let q = axum::extract::Query(std::collections::HashMap::new());
        let r1 = api_get_repo_tree(q).await.into_response();
        assert_eq!(r1.status(), StatusCode::OK);
        
        let r2 = api_get_repo_branches().await.into_response();
        assert!(r2.status() == StatusCode::OK || r2.status() == StatusCode::INTERNAL_SERVER_ERROR);
        
        let r3 = api_read_file_text().await.into_response();
        assert_eq!(r3.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[sealed_test]
    async fn test_get_api_incident_log_not_found() {
        let (tx, _) = tokio::sync::watch::channel(0);
        let (tx_updates, _) = tokio::sync::mpsc::unbounded_channel();
        let id_owner = crate::schema::identity_config::DidOwner { did: "d".to_string(), public_key_hex: "k".to_string(), private_key_hex: "pk".to_string() };
        let id = crate::schema::identity_config::Identity::Dreamer(id_owner);
        let ipc = crate::coordinator::ipc::IpcState {
            tx_ready: std::sync::Arc::new(tx),
            tx_updates: std::sync::Arc::new(tx_updates),
            shared_identity: std::sync::Arc::new(tokio::sync::RwLock::new(id)),
            token_market: crate::coordinator::market::ArbitrationMarket::new(crate::schema::coordinator_config::CoordinatorConfig::default()),
        };
        let ext = axum::extract::Extension(ipc);
        
        let td = tempfile::tempdir().unwrap();
        std::env::set_current_dir(td.path()).unwrap();
        
        // Without repo
        let resp = get_api_incident_log(ext.clone(), axum::extract::Path("dummy_log_ref".to_string())).await.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        
        // With repo but no branch
        git2::Repository::init(td.path()).unwrap();
        let resp2 = get_api_incident_log(ext, axum::extract::Path("dummy_log_ref".to_string())).await.into_response();
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_proxy_grinder_state_missing() {
        let path = axum::extract::Path("missing_did".to_string());
        let q = axum::extract::Query(std::collections::HashMap::new());
        let resp = proxy_grinder_state(path, q).await.into_response();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    #[sealed_test]
    async fn test_api_submit_task_error() {
        let (tx, _) = tokio::sync::watch::channel(0);
        let (tx_updates, _) = tokio::sync::mpsc::unbounded_channel();
        let id_owner = crate::schema::identity_config::DidOwner { did: "d".to_string(), public_key_hex: "k".to_string(), private_key_hex: "pk".to_string() };
        let id = crate::schema::identity_config::Identity::Dreamer(id_owner);
        let ipc = crate::coordinator::ipc::IpcState {
            tx_ready: std::sync::Arc::new(tx),
            tx_updates: std::sync::Arc::new(tx_updates),
            shared_identity: std::sync::Arc::new(tokio::sync::RwLock::new(id)),
            token_market: crate::coordinator::market::ArbitrationMarket::new(crate::schema::coordinator_config::CoordinatorConfig::default()),
        };
        let ext = axum::extract::Extension(ipc);
        let json = axum::Json(crate::schema::task::TaskRequestPayload { description: "t".to_string(), requestor: "u".to_string() });
        
        let td = tempfile::tempdir().unwrap();
        std::env::set_current_dir(td.path()).unwrap();
        let resp = api_submit_task(ext, json).await.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
