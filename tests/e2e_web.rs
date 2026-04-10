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
    
    // Explicitly register the server function for the test AXUM router
    leptos::server_fn::axum::register_explicit::<web::agents::GetActiveGrinders>();
    
    // 3. start web server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let _handle = spawn_web_server(listener);
    
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
    
    let statuses: Vec<web::schema::GrinderStatus> = serde_json::from_str(&text)
        .expect("Failed to deserialize GrinderStatus vector");
        
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
    let repo = git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init (provision 3 grinders)
    nancy::commands::init::init(tmp.path(), 3).await.unwrap();
    
    let identity_file = tmp.path().join(".nancy").join("identity.json");
    let root_id: nancy::schema::identity_config::Identity = serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    
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
        let statuses: Vec<web::schema::GrinderStatus> = serde_json::from_str(&text)
            .expect("Failed to deserialize GrinderStatus vector");
            
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
