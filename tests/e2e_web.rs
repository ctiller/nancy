use sealed_test::prelude::*;
use std::fs;
use tempfile::TempDir;
use nancy::coordinator::web::spawn_web_server;

mod common;

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
        token_market: nancy::coordinator::market::ArbitrationMarket::new(nancy::schema::coordinator_config::CoordinatorConfig::default()),
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
    
    let parsed: schema::GrindersResponse = serde_json::from_str(&text)
        .expect("Failed to deserialize GrindersResponse");
    let statuses = parsed.grinders;
        
    assert_eq!(statuses.len(), 4, "Expected 3 provisioned grinders + 1 dreamer from identity.json");
    for s in statuses {
        assert!(!s.is_online, "Agent {} should be offline", s.did);
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
    
    // 6. Poll reqwest agent list until 4 agents (3 grinders + 1 dreamer) are officially recorded ONLINE dynamically natively
    let mut online_count = 0;
    let mut attempts = 0;
    while online_count < 4 && attempts < 1000 {
        let res = client.get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .expect("Failed to execute reqwest");
            
        let status = res.status();
        assert_eq!(status, reqwest::StatusCode::OK, "API returned non-200");
        
        let text = res.text().await.unwrap();
        let parsed: schema::GrindersResponse = serde_json::from_str(&text)
            .expect("Failed to deserialize GrindersResponse");
        let statuses = parsed.grinders;
            
        online_count = statuses.iter().filter(|s| s.is_online).count();
        if online_count < 4 {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            attempts += 1;
        } else {
            break;
        }
    }
    
    assert_eq!(online_count, 4, "Failed to observe 4 ONLINE Agents before timeout");
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
    while !online && attempts < 1000 {
        let res = client.get(&format!("{}/api/grinders", base_url))
            .header("Accept", "application/json")
            .send().await.unwrap();
            
        let parsed: schema::GrindersResponse = res.json().await.unwrap();
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
            
        let parsed: schema::GrindersResponse = res.json().await.unwrap();
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
    
    git2::Repository::init(tmp.path()).unwrap();
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();
    
    let (mock_port, test_queue) = common::mock_gemini::spawn_mock_server().await;
    
    // We expect the Coordinator to query the orchestrator, and the Grinder to resolve the Introspection Plan organically natively.
    // For a generic generic "Test Topology Request", let's mock the grinder LLM calls!
    common::mock_gemini::push_tool_call_response(&test_queue, "report_completed_task", serde_json::json!({
        "analysis": "Completed Test Topology Request",
        "conclusion": "Topology is rendered.",
    })).await;
    
    unsafe {
        std::env::set_var("GEMINI_API_BASE_URL", format!("http://127.0.0.1:{}", mock_port));
        std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy"));
    }

    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    
    // Post authentic task payload securely resolving orchestrator loops natively!
    let task_payload = serde_json::json!({ "requestor": "tester", "description": "Test Topology Request" });
    let post_res = client.post(&format!("http://127.0.0.1:{}/api/tasks", port))
        .header("Content-Type", "application/json")
        .json(&task_payload)
        .send()
        .await.unwrap();
    assert_eq!(post_res.status(), reqwest::StatusCode::OK);

    let url = format!("http://127.0.0.1:{}/api/tasks/topology", port);
    
    let mut found = false;
    let mut attempts = 0;
    while !found && attempts < 50 {
        let res = client.get(&url).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        
        let text = res.text().await.unwrap();
        let parsed: schema::TopologyResponse = serde_json::from_str(&text).unwrap();
        
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
    
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_tasks_evaluations() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    let repo = git2::Repository::init(tmp.path()).unwrap();
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();
    
    let (mock_port, test_queue) = common::mock_gemini::spawn_mock_server().await;
    
    // Manually log an Ask event to verify Dreamer evaluation plumbing independently of Grinder tool-call timing
    {
        let id_obj = nancy::schema::identity_config::Identity::load(tmp.path()).await.unwrap();
        let writer = nancy::events::writer::Writer::new(&repo, id_obj.clone()).unwrap();
        writer.log_event(nancy::schema::registry::EventPayload::Ask(nancy::schema::task::AskPayload {
            item_ref: "test_ask".to_string(),
            question: "Is this working?".to_string(),
            agent_path: "coord".to_string(),
            task_name: "Test Task".to_string(),
        })).unwrap();
        writer.commit_batch().unwrap();
        
        // Assert it wrote!
        let reader = nancy::events::reader::Reader::new(&repo, id_obj.get_did_owner().did.clone());
        let mut count = 0;
        for _ in reader.iter_events().unwrap() { count += 1; }
        println!("TEST SANITY: coordinator has {} events natively logged BEFORE boot.", count);
    }

    // We expect the grinder to evaluate the task naturally via mock LLM output.
    common::mock_gemini::push_tool_call_response(&test_queue, "ask_human", serde_json::json!({
        "question": "Can you evaluate this task?",
    })).await;
    // Provide plenty of responses for both grinder follow-ups and dreamer evaluations
    for _ in 0..10 {
        common::mock_gemini::push_text_response(&test_queue, "95").await;
    }
    
    unsafe {
        std::env::set_var("GEMINI_API_BASE_URL", format!("http://127.0.0.1:{}", mock_port));
        std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy"));
    }

    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    
    let task_payload = serde_json::json!({ "requestor": "tester", "description": "Test Topology Request" });
    let post_res = client.post(&format!("http://127.0.0.1:{}/api/tasks", port))
        .header("Content-Type", "application/json")
        .json(&task_payload)
        .send()
        .await.unwrap();
    assert_eq!(post_res.status(), reqwest::StatusCode::OK);

    let url = format!("http://127.0.0.1:{}/api/tasks/evaluations", port);
    
    let mut found = false;
    let mut attempts = 0;
    while !found && attempts < 150 {
        let res = client.get(&url).send().await.unwrap();
        assert_eq!(res.status(), reqwest::StatusCode::OK);
        
        let text = res.text().await.unwrap();
        let payload: serde_json::Value = serde_json::from_str(&text).unwrap();
        let parsed: Vec<schema::TaskEvaluation> = serde_json::from_value(payload["evaluations"].clone()).unwrap();
        
        if parsed.iter().any(|e| e.score > 0) {
            found = true;
        } else {
            if attempts % 10 == 0 {
                println!("Attempt {}: No evaluations found yet: {:?}", attempts, payload);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    if !found {
        let sock_dir = std::path::PathBuf::from(".nancy/sockets");
        if let Ok(entries) = std::fs::read_dir(sock_dir) {
            for entry in entries.flatten() {
                let log_path = entry.path().join("container.log");
                if let Ok(logs) = std::fs::read_to_string(&log_path) {
                    println!("==== CONTAINER {} LOGS ====\n{}\n===================", entry.path().display(), logs);
                }
            }
        }
    }
    
    assert!(found, "Failed to observe Test TaskEvaluation in evaluations response natively dynamically before timeout: Final response was {:?}", client.get(&url).send().await.unwrap().text().await.unwrap());
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_human_pending_standalone_grinder_hydration() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    let repo = git2::Repository::init(tmp.path()).unwrap();
    nancy::commands::init::init(tmp.path(), 0).await.unwrap();
    
    // Simulate a Standalone Grinder natively writing to its own DID branch securely!
    let standalone_identity = nancy::schema::identity_config::Identity::Grinder( 
        nancy::schema::identity_config::DidOwner::generate()
    );
    
    let grinder_writer = nancy::events::writer::Writer::new(&repo, standalone_identity).unwrap();
    grinder_writer.log_event(nancy::schema::registry::EventPayload::ReviewPlan(
        nancy::schema::task::ReviewPlanPayload {
            plan_ref: "standalone_plan_xyz123".to_string(),
            agent_path: "planning".to_string(),
            task_name: "Standalone Hydration Test".to_string(),
            document: serde_json::from_value(serde_json::json!({
                "title": "Title",
                "summary": "Sum",
                "background_context": "",
                "goals": [],
                "non_goals": [],
                "proposed_design": [],
                "risks_and_tradeoffs": [],
                "alternatives_considered": [],
                "recorded_dissents": []
            })).unwrap(),
        }
    )).unwrap();
    grinder_writer.commit_batch().unwrap();

    // Boot the Coordinator!
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);

    // Call /api/human/pending
    let res = client.get(&format!("{}/api/human/pending", base_url))
        .header("Accept", "application/json")
        .send().await.unwrap();
        
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    
    let pending_json: serde_json::Value = res.json().await.unwrap();
    let plan_reviews = pending_json["plan_reviews"].as_array().expect("plan_reviews missing or not an array");
    
    assert_eq!(plan_reviews.len(), 1, "AppView Hydration missed the standalone grinder's Git branch!");
    let payload = &plan_reviews[0];
    assert_eq!(payload["plan_ref"], "standalone_plan_xyz123");
    assert_eq!(payload["task_name"], "Standalone Hydration Test");
}

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_web_market_state() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    // 1. git init
    let _repo = git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init 
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();
    
    // 3. Boot coordinator
    unsafe { std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy")); }
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path()).await.unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    
    tokio::spawn(async move {
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });
    
    let port = rx.await.expect("Coordinator boot dropped callback!");
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    
    // Request market state
    let res = client.get(&format!("{}/api/market/state", base_url))
        .send().await.unwrap();
        
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    
    // Spot Market should have active quotas hydrated organically natively.
    let market_state: serde_json::Value = res.json().await.unwrap();
    let per_model_stats = market_state["per_model_stats"].as_object().expect("Expected per_model_stats map in deserialized payload structurally");
    assert!(!per_model_stats.is_empty(), "Spot Market should have initially hydrated limits recorded defensively");
}
