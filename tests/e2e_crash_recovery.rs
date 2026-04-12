use sealed_test::prelude::*;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
#[sealed_test(env = [
    ("GEMINI_API_KEY", "mock")
])]
async fn test_e2e_crash_recovery() {
    let tmp = TempDir::new().unwrap();
    std::env::set_current_dir(tmp.path()).unwrap();
    
    // 1. git init
    let repo = git2::Repository::init(tmp.path()).unwrap();
    
    // 2. nancy init (provision 1 grinder)
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();
    
    let identity_file = tmp.path().join(".nancy").join("identity.json");
    let root_id: nancy::schema::identity_config::Identity = serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    let root_did = root_id.get_did_owner().did.clone();
    
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
    
    // Wait for the single grinder to come online natively
    let mut target_grinder_did = String::new();
    let mut attempts = 0;
    while target_grinder_did.is_empty() && attempts < 200 {
        let res = client.get(&url).header("Accept", "application/json").send().await.unwrap();
        let text = res.text().await.unwrap();
        let parsed: schema::GrindersResponse = serde_json::from_str(&text).unwrap();
        
        if let Some(target) = parsed.grinders.into_iter().find(|s| s.is_online && s.agent_type == "grinder") {
            target_grinder_did = target.did;
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }
    }
    
    assert!(!target_grinder_did.is_empty(), "Failed to observe ONLINE Grinder before timeout");

    // Invoke /crash forcefully!
    let grinder_socket_path = tmp.path().join(".nancy").join("sockets").join(&target_grinder_did).join("grinder.sock");
    
    let uds_client = reqwest::Client::builder().unix_socket(grinder_socket_path).build().unwrap();
    let _crash_res = uds_client.post("http://localhost/crash").send().await;
    // Expected to error or succeed (depending on if it crashed cleanly before sending response)
    
    // Now wait for an AgentCrashReport in the Coordinator event stream
    let mut crash_ref = String::new();
    attempts = 0;
    while crash_ref.is_empty() && attempts < 200 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let reader = nancy::events::reader::Reader::new(&repo, root_did.clone());
        if let Ok(iter) = reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    if let nancy::schema::registry::EventPayload::AgentCrashReport(report) = env.payload {
                        if report.crashing_agent_did == target_grinder_did {
                            crash_ref = report.log_ref;
                            break;
                        }
                    }
                }
            }
        }
        attempts += 1;
    }
    
    assert!(!crash_ref.is_empty(), "Failed to capture AgentCrashReport payload dynamically!");
    
    // Verify the log was natively physically attached into an incidents/ tree
    let branch_name = format!("refs/heads/nancy/{}", root_did.replace(":", "_"));
    let latest_commit = repo.find_reference(&branch_name).unwrap().peel_to_commit().unwrap();
    let tree = latest_commit.tree().unwrap();
    let incidents_tree = tree.get_name("incidents").expect("Found no incidents/ folder").to_object(&repo).unwrap().into_tree().unwrap();
    let log_blob = incidents_tree.get_name(&crash_ref).expect("Found no referenced log file bounds").to_object(&repo).unwrap().into_blob().unwrap();
    let text = std::str::from_utf8(log_blob.content()).unwrap();
    assert!(text.contains("Intentionally invoked /crash route via IPC! Aborting process instantly"), "Expected crash text in output stream, got: {}", text);

    // Verify it organically comes back online! We enforce base_delay=5s + max 1s jitter so maybe wait up to 10s safely
    attempts = 0;
    let mut online_again = false;
    while !online_again && attempts < 200 {
        let res = client.get(&url).header("Accept", "application/json").send().await.unwrap();
        let text = res.text().await.unwrap();
        let parsed: schema::GrindersResponse = serde_json::from_str(&text).unwrap();
        
        if let Some(target) = parsed.grinders.into_iter().find(|s| s.did == target_grinder_did) {
            if target.is_online {
                online_again = true;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }
    assert!(online_again, "Failed to observe grinder come back online after its dynamic randomized exponential backoff.");
}
