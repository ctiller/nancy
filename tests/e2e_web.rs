use sealed_test::prelude::*;
use std::fs;
use tempfile::TempDir;
use nancy::coordinator::web::spawn_web_server;

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_grinder_list() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    // 1. git init
    git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init (provision 3 grinders)
    nancy::commands::init::init(tmp.path(), 3).await.unwrap();
    
    // Explicit leptos bindings obsolete: Grinder endpoint migrated to pure Axum handling organically.
    
    // 3. start web server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let (tx_ready, _) = tokio::sync::watch::channel(0);
    let (tx_updates, _) = tokio::sync::mpsc::unbounded_channel();
    let loaded_identity = nancy::schema::identity_config::Identity::load(tmp.path()).await.unwrap();
    let ipc_state = nancy::coordinator::ipc::IpcState {
        tx_ready: std::sync::Arc::new(tx_ready),
        tx_updates: std::sync::Arc::new(tx_updates),
        shared_identity: std::sync::Arc::new(tokio::sync::RwLock::new(loaded_identity)),
    };
    let _handle = spawn_web_server(listener, ipc_state);
    
    // Allow server to boot explicitly cleanly
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    
    // 4. reqwest the agent list
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/grinders", local_addr.port());
    println!("Requesting: {}", url);
    
    let res = client.get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .expect("Failed to execute reqwest");
        
    let status = res.status();
    let text = res.text().await.unwrap();
    if status != reqwest::StatusCode::OK {
        println!("API failed! Response: {}", text);
    }
    assert_eq!(status, reqwest::StatusCode::OK, "API returned non-200");
    
    let parsed: web::schema::GrindersResponse = serde_json::from_str(&text)
        .expect("Failed to deserialize GrindersResponse");
    let statuses = parsed.grinders;
        
    assert_eq!(statuses.len(), 3, "Expected 3 provisioned grinders from identity.json");
    for s in statuses {
        assert!(!s.is_online, "Grinder {} should be offline", s.did);
    }
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_grinders_online() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    // 1. git init
    let _repo = git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init (provision 3 grinders)
    nancy::commands::init::init(tmp.path(), 3).await.unwrap();
    
    let identity_file = tmp.path().join(".nancy").join("identity.json");
    let _root_id: nancy::schema::identity_config::Identity = serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    
    // Secure authentic nancy executable mappings natively for tests running `DockerOrchestrator` organically 
    unsafe { std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy")); }
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        // Run coordinator indefinitely with bound port callback wrapper
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    // Acquire the dynamic local port published via the socket bind callback
    let port = rx.await.expect("Coordinator boot dropped callback!");
    
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/grinders", port);
    
    // 6. Poll reqwest agent list until 3 grinders are officially recorded ONLINE dynamically natively
    let mut online_count = 0;
    let mut attempts = 0;
    while online_count < 3 && attempts < 200 {
        let res = client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .expect("Failed to execute reqwest");
            
        let status = res.status();
        assert_eq!(status, reqwest::StatusCode::OK, "API returned non-200");
        
        let text = res.text().await.unwrap();
        let parsed: web::schema::GrindersResponse = serde_json::from_str(&text)
            .expect("Failed to deserialize GrindersResponse");
        let statuses = parsed.grinders;
            
        online_count = statuses.iter().filter(|s| s.is_online).count();
        if online_count < 3 {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        } else {
            break;
        }
    }
    
    assert_eq!(online_count, 3, "Failed to observe 3 ONLINE Grinders before timeout");
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_add_remove_grinder() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    // 1. git init
    let _repo = git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init (provision 0 grinders)
    nancy::commands::init::init(tmp.path(), 0).await.unwrap();
    
    // Secure authentic nancy executable mappings natively for tests running `DockerOrchestrator` organically 
    unsafe { std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy")); }
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        // Run coordinator indefinitely with bound port callback wrapper
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    // Acquire the dynamic local port published via the socket bind callback
    let port = rx.await.expect("Coordinator boot dropped callback!");
    
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    
    // 3. POST /api/add-grinder
    let add_res = client.post(&format!("{}/api/add-grinder", base_url))
        .send().await.expect("Failed to POST add-grinder");
        
    assert_eq!(add_res.status(), reqwest::StatusCode::OK);
    
    let add_json: serde_json::Value = add_res.json().await.unwrap();
    let added_did = add_json["did"].as_str().expect("Add Grinder response missing did").to_string();
    
    // 4. Poll /api/grinders until the target is online
    let mut online = false;
    let mut attempts = 0;
    while !online && attempts < 200 {
        let res = client.get(&format!("{}/api/grinders", base_url))
            .header("Accept", "application/json")
            .send().await.unwrap();
            
        let parsed: web::schema::GrindersResponse = res.json().await.unwrap();
        let statuses = parsed.grinders;
        if let Some(target) = statuses.iter().find(|s| s.did == added_did) {
            online = target.is_online;
        }
        
        if !online {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    assert!(online, "Failed to observe newly added Grinder {} as ONLINE before timeout", added_did);
    
    // 5. POST /api/remove-grinder
    let remove_payload = serde_json::json!({ "did": added_did });
    let remove_res = client.post(&format!("{}/api/remove-grinder", base_url))
        .header("Content-Type", "application/json")
        .json(&remove_payload)
        .send().await.expect("Failed to POST remove-grinder");
        
    assert_eq!(remove_res.status(), reqwest::StatusCode::OK);
    
    // 6. Poll /api/grinders until the target is gone
    let mut is_gone = false;
    attempts = 0;
    while !is_gone && attempts < 200 {
        let res = client.get(&format!("{}/api/grinders", base_url))
            .header("Accept", "application/json")
            .send().await.unwrap();
            
        let parsed: web::schema::GrindersResponse = res.json().await.unwrap();
        let statuses = parsed.grinders;
        is_gone = !statuses.iter().any(|s| s.did == added_did);
        
        if !is_gone {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    assert!(is_gone, "Failed to observe Grinder {} removal before timeout", added_did);
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_tasks_topology() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    let repo = git2::Repository::init(tmp.path()).unwrap();
    nancy::commands::init::init(tmp.path(), 0).await.unwrap();
    
    let identity_file = tmp.path().join(".nancy").join("identity.json");
    let root_id: nancy::schema::identity_config::Identity = serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    
    let writer = nancy::events::writer::Writer::new(&repo, root_id).unwrap();
    writer.log_event(nancy::schema::registry::EventPayload::TaskRequest(
        nancy::schema::task::TaskRequestPayload {
            requestor: "tester".to_string(),
            description: "Test Topology Request".to_string(),
        }
    )).unwrap();
    writer.commit_batch().unwrap();

    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/tasks/topology", port);
    
    let mut found = false;
    let mut attempts = 0;
    while !found && attempts < 50 {
        let res = client.get(&url).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        
        let text = res.text().await.unwrap();
        let parsed: web::schema::TopologyResponse = serde_json::from_str(&text).unwrap();
        
        if parsed.nodes.iter().any(|n| n.name == "Test Topology Request") {
            assert!(parsed.max_width > 0.0, "Expected max_width to be populated by backend dugong layout");
            assert!(parsed.max_height > 0.0, "Expected max_height to be populated by backend dugong layout");
            found = true;
        } else {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    assert!(found, "Failed to observe Test Topology Request in topology response");
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_incident_logs() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    let repo = git2::Repository::init(tmp.path()).unwrap();
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();
    let identity = nancy::schema::identity_config::Identity::load(tmp.path()).await.unwrap();
    
    let did = match &identity {
        nancy::schema::identity_config::Identity::Coordinator { workers, .. } => workers[0].did.clone(),
        _ => panic!("Not a coordinator"),
    };
    
    let writer = nancy::events::writer::Writer::new(&repo, identity.clone()).unwrap();
    writer.attach_incident_log("test-crash.log", "thread '<unnamed>' panicked at 'explicit panic'");
    writer.log_event(nancy::schema::registry::EventPayload::AgentCrashReport(
        nancy::schema::task::AgentCrashReportPayload {
            crashing_agent_did: did.clone(),
            log_ref: "test-crash.log".to_string(),
            next_restart_at_unix: Some(1234),
            failures: Some(1),
        }
    )).unwrap();
    writer.commit_batch().unwrap();

    // Secure authentic nancy executable mappings natively for tests running `DockerOrchestrator` organically 
    unsafe { std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy")); }
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    
    let mut found = false;
    let mut attempts = 0;
    while !found && attempts < 20 {
        let url = format!("http://127.0.0.1:{}/api/incidents/test-crash.log", port);
        if let Ok(res) = client.get(&url).send().await {
            if res.status() == reqwest::StatusCode::OK {
                let text = res.text().await.unwrap();
                assert!(text.contains("explicit panic"), "Log text didn't match. Got: {}", text);
                found = true;
            } else {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                attempts += 1;
            }
        } else {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    assert!(found, "Failed to retrieve incident log via Web API");
}
