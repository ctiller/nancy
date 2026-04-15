// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
    let async_repo = nancy::git::AsyncRepository::discover(repo.workdir().unwrap())
        .await
        .unwrap();

    // 2. nancy init (provision 1 doer)
    nancy::commands::init::init(tmp.path(), 1).await.unwrap();

    let identity_file = tmp.path().join(".nancy").join("identity.json");
    let root_id: nancy::schema::identity_config::Identity =
        serde_json::from_str(&fs::read_to_string(&identity_file).unwrap()).unwrap();
    let root_did = root_id.get_did_owner().did.clone();

    // Map nancy executable for tests running `DockerOrchestrator`
    unsafe {
        std::env::set_var("NANCY_E2E_EXECUTABLE", env!("CARGO_BIN_EXE_nancy"));
    }
    let mut coord = nancy::commands::coordinator::Coordinator::new(tmp.path())
        .await
        .unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();

    tokio::spawn(async move {
        // Run coordinator indefinitely with bound port callback wrapper
        let _ = coord.run_until(0, Some(tx), |_| false).await;
    });

    // Acquire the dynamic local port published via the socket bind callback
    let port = rx.await.expect("Coordinator boot dropped callback!");

    // Wait for HTTPS asynchronous socket rebinding to finish
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .unwrap();
    let url = format!("https://127.0.0.1:{}/api/doers", port);

    // Wait for the single doer to come online
    let mut target_grinder_did = String::new();
    let mut attempts = 0;
    while target_grinder_did.is_empty() && attempts < 200 {
        let res = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .unwrap();
        let text = res.text().await.unwrap();
        let parsed: schema::DoersResponse = serde_json::from_str(&text).unwrap();

        if let Some(target) = parsed
            .doers
            .into_iter()
            .find(|s| s.is_online && s.agent_type == "doer")
        {
            target_grinder_did = target.did;
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }
    }

    assert!(
        !target_grinder_did.is_empty(),
        "Failed to observe ONLINE Grinder before timeout"
    );

    // Invoke /crash forcefully!
    let grinder_socket_path = tmp
        .path()
        .join(".nancy")
        .join("sockets")
        .join(&target_grinder_did)
        .join("doer.sock");

    let uds_client = reqwest::Client::builder()
        .unix_socket(grinder_socket_path)
        .build()
        .unwrap();
    let _crash_res = uds_client.post("http://localhost/crash").send().await;
    // Expected to error or succeed (depending on if it crashed cleanly before sending response)

    // Now wait for an AgentCrashReport in the Coordinator event stream
    let mut crash_ref = String::new();
    attempts = 0;
    while crash_ref.is_empty() && attempts < 200 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let reader = nancy::events::reader::Reader::new(&async_repo, root_did.clone());
        if let Ok(iter) = reader.iter_events().await {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    if let nancy::schema::registry::EventPayload::AgentCrashReport(report) =
                        env.payload
                    {
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

    assert!(
        !crash_ref.is_empty(),
        "Failed to capture AgentCrashReport payload dynamically!"
    );

    // Verify the log was attached to an incidents/ tree
    let branch_name = format!("refs/heads/nancy/{}", root_did.replace(":", "_"));
    let latest_commit = repo
        .find_reference(&branch_name)
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let tree = latest_commit.tree().unwrap();
    let incidents_tree = tree
        .get_name("incidents")
        .expect("Found no incidents/ folder")
        .to_object(&repo)
        .unwrap()
        .into_tree()
        .unwrap();
    let log_blob = incidents_tree
        .get_name(&crash_ref)
        .expect("Found no referenced log file bounds")
        .to_object(&repo)
        .unwrap()
        .into_blob()
        .unwrap();
    let text = std::str::from_utf8(log_blob.content()).unwrap();
    assert!(
        text.contains("Intentionally invoked /crash route via IPC! Aborting process instantly"),
        "Expected crash text in output stream, got: {}",
        text
    );

    // Verify it organically comes back online! We enforce base_delay=5s + max 1s jitter so maybe wait up to 10s safely
    attempts = 0;
    let mut online_again = false;
    while !online_again && attempts < 200 {
        let res = client
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .unwrap();
        let text = res.text().await.unwrap();
        let parsed: schema::DoersResponse = serde_json::from_str(&text).unwrap();

        if let Some(target) = parsed
            .doers
            .into_iter()
            .find(|s| s.did == target_grinder_did)
        {
            if target.is_online {
                online_again = true;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attempts += 1;
    }
    assert!(
        online_again,
        "Failed to observe grinder come back online after its dynamic randomized exponential backoff."
    );
}

// DOCUMENTED_BY: [docs/adr/0068-execute-task-test-module-migration.md]
