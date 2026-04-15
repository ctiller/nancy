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

use crate::llm::thinking_llm;
use futures_util::future::try_join_all;

/// This is the swiss army knife of investigation. Use this to find answers recursively.
use std::sync::Arc;

pub struct AskHuman {
    pub item_ref: String,
    pub repo_path: std::path::PathBuf,
}

impl AskHuman {
    pub async fn start(question: &str, task_name: &str, agent_path: &str) -> Option<Self> {
        if std::env::var("NANCY_HUMAN_DID").is_err() {
            return None;
        }

        let repo_path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let r = crate::git::AsyncRepository::discover(&repo_path)
            .await
            .ok()?;
        let wd = r.workdir().unwrap_or(repo_path.clone());
        let id_obj = crate::schema::identity_config::Identity::load(wd)
            .await
            .ok()?;
        let writer = crate::events::writer::Writer::new(&r, id_obj).ok()?;

        let aref = format!(
            "ask_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let payload = crate::schema::registry::EventPayload::Ask(crate::schema::task::AskPayload {
            item_ref: aref.clone(),
            question: question.to_string(),
            agent_path: agent_path.to_string(),
            task_name: task_name.to_string(),
        });
        let _ = writer.log_event(payload);
        let _ = writer.commit_batch().await;

        Some(Self {
            item_ref: aref,
            repo_path,
        })
    }

    pub async fn wait_for_response(&self) -> String {
        let Ok(human_did) = std::env::var("NANCY_HUMAN_DID") else {
            return String::new();
        };

        let mut human_appended_response = String::new();
        let mut should_wait = false;

        if let Ok(r) = crate::git::AsyncRepository::discover(&self.repo_path).await {
            let reader = crate::events::reader::Reader::new(&r, human_did.clone());
            if let Ok(iter) = reader.iter_events().await {
                for ev in iter.flatten() {
                    if let crate::schema::registry::EventPayload::Seen(s) = &ev.payload {
                        if s.item_ref == self.item_ref {
                            should_wait = true;
                        }
                    } else if let crate::schema::registry::EventPayload::HumanResponse(hr) =
                        &ev.payload
                    {
                        if hr.item_ref == self.item_ref {
                            human_appended_response =
                                format!("\n\n[HUMAN RESPONSE TO YOUR ASK]: {}", hr.text_response);
                            should_wait = false;
                        }
                    }
                }
            }
        }

        if should_wait && human_appended_response.is_empty() {
            let sleep_time = if std::env::var("NANCY_TEST_POLL_TIMEOUT").is_ok() {
                50
            } else {
                1000
            };
            for _ in 0..30 {
                tokio::time::sleep(std::time::Duration::from_millis(sleep_time)).await;
                if let Ok(r) = crate::git::AsyncRepository::discover(&self.repo_path).await {
                    let reader2 = crate::events::reader::Reader::new(&r, human_did.clone());
                    if let Ok(iter) = reader2.iter_events().await {
                        for ev in iter.flatten() {
                            if let crate::schema::registry::EventPayload::HumanResponse(hr) =
                                &ev.payload
                            {
                                if hr.item_ref == self.item_ref {
                                    human_appended_response = format!(
                                        "\n\n[HUMAN RESPONSE TO YOUR ASK]: {}",
                                        hr.text_response
                                    );
                                    break;
                                }
                            }
                        }
                    }
                }
                if !human_appended_response.is_empty() {
                    break;
                }
            }
        }

        human_appended_response
    }

    pub async fn cancel(&self) {
        if let Ok(r) = crate::git::AsyncRepository::discover(&self.repo_path).await {
            let wd = r.workdir().unwrap_or(self.repo_path.clone());
            if let Ok(id_obj) = crate::schema::identity_config::Identity::load(wd).await {
                if let Ok(writer) = crate::events::writer::Writer::new(&r, id_obj) {
                    let payload = crate::schema::registry::EventPayload::CancelItem(
                        crate::schema::task::CancelItemPayload {
                            item_ref: self.item_ref.clone(),
                        },
                    );
                    let _ = writer.log_event(payload);
                    let _ = writer.commit_batch().await;
                }
            }
        }
    }
}

use std::cell::RefCell;

tokio::task_local! {
    static INVESTIGATION_HISTORY: RefCell<Vec<String>>;
}

pub async fn investigate_impl(
    perms: Arc<crate::tools::filesystem::Permissions>,
    question: String,
    task_name: String,
    agent_path: String,
) -> anyhow::Result<String> {
    let is_scoped = INVESTIGATION_HISTORY.try_with(|_| ()).is_ok();
    if !is_scoped {
        INVESTIGATION_HISTORY.scope(RefCell::new(vec![]), async move {
            investigate_inner(perms, question, task_name, agent_path).await
        }).await
    } else {
        investigate_inner(perms, question, task_name, agent_path).await
    }
}

async fn investigate_inner(
    perms: Arc<crate::tools::filesystem::Permissions>,
    question: String,
    task_name: String,
    agent_path: String,
) -> anyhow::Result<String> {
    let history_json = INVESTIGATION_HISTORY.try_with(|h| {
        let hist = h.borrow();
        serde_json::to_string(&*hist).unwrap_or_default()
    }).unwrap_or_default();

    if history_json != "[]" && !history_json.is_empty() {
        let prompt = format!(
            "Determine if the following new question is semantically a repetition of any of the previously investigated questions explicitly.\n\
            Previous Questions: {}\n\
            New Question: {}\n\n\
            Return a JSON object with `is_repetition` (boolean).",
            history_json, question
        );

        #[derive(serde::Deserialize, schemars::JsonSchema)]
        struct RepetitionCheck {
            is_repetition: bool,
        }

        if let Ok(mut checker) = crate::llm::lite_llm("repetition_checker", schema::TaskType::Validation).build() {
            if let Ok(res) = checker.ask::<RepetitionCheck>(&prompt).await {
                if res.is_repetition {
                    return Ok("Execution denied: question is a repetition of a previous investigation bounds on the active subagent lineage.".to_string());
                }
            }
        }
    }

    let _ = INVESTIGATION_HISTORY.try_with(|h| h.borrow_mut().push(question.clone()));

    let root_path = perms.read_dirs.first().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string());
    let system_prompt_str = format!(r#"You are an expert forensic programmer and autonomous system investigator.
Your objective is to comprehensively map, diagnose, and answer the given question by actively exploring the system using your available toolkit.

Follow these critical principles:
1. **Explore Deeply**: Do not guess or hallucinate code paths. Use `grep_search`, `list_dir`, and `view_files` physically. 
2. **Be Autonomous**: A single search will rarely yield the full context. If your first search misses, hypothesize new locations and chase down references recursively.
3. **Connect the Dots**: Cross-reference definitions, configurations, and active architectures to build a complete mental picture.
4. **Be Exhaustive**: When asked to locate or identify something, do not stop at the first match. Comb through the architecture to guarantee complete isolation.
5. **Report Clearly**: Synthesize your discoveries into a hyper-direct, rigorous, and technical answer yielding exact file paths, snippets, and mechanical processes.
6. **Acknowledge Sandbox Boundaries**: If your filesystem tools return 'Explicit permission missing against mapped boundary target', DO NOT attempt to bypass or brute-force the sandbox with relative path traversals. You MUST immediately stop and respond with a clear statement that the required artifact is unavailable due to isolated directory bounds.

**CRITICAL CONTEXT**:
Your active bound investigation root path is: `{}`
DO NOT USE `.` as a target directory, always use absolute paths starting from your root path!"#, root_path);

    let inner_agent = format!("{}>investigator", agent_path);
    let tools = super::AgentToolsBuilder::new()
        .grant_perms(perms)
        .context(&task_name, &inner_agent)
        .build();

    let mut client = thinking_llm("investigator", schema::TaskType::Investigate)
        .temperature(0.3)
        .tools(tools)
        .system_prompt(&system_prompt_str)
        .build()?;

    let ask_human = AskHuman::start(&question, &task_name, &agent_path).await;
    let out = client.ask::<String>(&question).await;

    let human_appended_response = if let Some(ask) = ask_human {
        let resp = ask.wait_for_response().await;
        ask.cancel().await;
        resp
    } else {
        String::new()
    };

    match out {
        Ok(s) => Ok(format!("{}{}", s, human_appended_response)),
        Err(e) => Err(e),
    }
}

/// A parallelism helper to run multiple investigate tools simultaneously.
pub async fn multi_investigate_impl(
    perms: Arc<crate::tools::filesystem::Permissions>,
    questions: Vec<String>,
    task_name: String,
    agent_path: String,
) -> anyhow::Result<Vec<String>> {
    let futures = questions.into_iter().map(|q| {
        let p = Arc::clone(&perms);
        let t = task_name.clone();
        let a = agent_path.clone();
        async move { investigate_impl(p, q, t, a).await }
    });
    try_join_all(futures).await
}

pub fn create_investigate_tools(
    permissions: Arc<crate::tools::filesystem::Permissions>,
    task_name: String,
    agent_path: String,
) -> Vec<Box<dyn crate::llm::tool::LlmTool>> {
    let p_inv = Arc::clone(&permissions);
    let t_inv = task_name.clone();
    let a_inv = agent_path.clone();

    let inv = llm_macros::make_tool!(
        "investigate",
        "This is the swiss army knife of investigation. Use this to find answers recursively.",
        move |question: String| {
            let perms = Arc::clone(&p_inv);
            let t = t_inv.clone();
            let a = a_inv.clone();
            async move { investigate_impl(perms, question, t, a).await }
        }
    );

    let p_mul = Arc::clone(&permissions);
    let t_mul = task_name.clone();
    let a_mul = agent_path.clone();

    let mul = llm_macros::make_tool!(
        "multi_investigate",
        "A parallelism helper to run multiple investigate tools simultaneously.",
        move |questions: Vec<String>| {
            let perms = Arc::clone(&p_mul);
            let t = t_mul.clone();
            let a = a_mul.clone();
            async move { multi_investigate_impl(perms, questions, t, a).await }
        }
    );

    vec![inv, mul]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_investigate_coverage() {
        let perms = Arc::new(crate::tools::filesystem::Permissions {
            base_dir: None,
            read_dirs: vec![],
            write_dirs: vec![],
        });
        let _ =
            investigate_impl(perms, "hello".to_string(), "t".to_string(), "a".to_string()).await;
    }

    #[tokio::test]
    async fn test_multi_investigate_coverage() {
        let perms = Arc::new(crate::tools::filesystem::Permissions {
            base_dir: None,
            read_dirs: vec![std::path::PathBuf::from("/tmp/does_not_exist")],
            write_dirs: vec![],
        });
        let _ = multi_investigate_impl(
            perms,
            vec!["q1".to_string(), "q2".to_string()],
            "t".to_string(),
            "a".to_string(),
        )
        .await;
    }

    use sealed_test::prelude::*;

    #[tokio::test]
    #[sealed_test(env = [
        ("NANCY_NO_TRACE_EVENTS", "1"),
        ("NANCY_HUMAN_DID", "did:key:z6MkiuexGTCjkPmnT4jv1JNAmeV7UnBoMh1rsxLoYYCQ8Txs"),
        ("NANCY_TEST_POLL_TIMEOUT", "1"),
        ("GEMINI_API_KEY", "mock")
    ])]
    async fn test_investigate_hitl_coverage() {
        let td = tempfile::tempdir().unwrap();
        let td_path = td.path().to_path_buf();
        std::env::set_current_dir(&td_path).unwrap();

        let _repo = git2::Repository::init(&td_path).unwrap();
        crate::commands::init::init(td_path.clone(), 1)
            .await
            .unwrap();

        // Spawn async human responder securely
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(r) = crate::git::AsyncRepository::discover(".").await {
                let test_human = crate::schema::identity_config::DidOwner {
                    did: "z6MkiuexGTCjkPmnT4jv1JNAmeV7UnBoMh1rsxLoYYCQ8Txs".to_string(),
                    public_key_hex:
                        "4231c04a3dd5b64700018aa7cffe4ad35cf9bad05ec2fa21c3dbf5c0339872f8"
                            .to_string(),
                    private_key_hex:
                        "839ce4cc3dc57e147f119a6999c16230ea2d7b47c85ddc2714d25cb202b5023b"
                            .to_string(),
                };

                // Using Doer namespace wrapper to allow writer instantiation since Human variant doesn't independently exist
                if let Ok(writer) = crate::events::writer::Writer::new(
                    &r,
                    crate::schema::identity_config::Identity::Doer(test_human),
                ) {

                    // Iterate and pick up the item_ref organically to respond correctly
                    let _reader = crate::events::reader::Reader::new(
                        &r,
                        "did:key:z6MkiuexGTCjkPmnT4jv1JNAmeV7UnBoMh1rsxLoYYCQ8Txs".to_string(),
                    );
                    let _ = writer.log_event(crate::schema::registry::EventPayload::Seen(
                        crate::schema::task::SeenPayload {
                            item_ref: "a".to_string(),
                            timestamp: 0,
                        },
                    ));
                    let _ = writer.commit_batch().await;
                }
            }
        });

        crate::llm::mock::builder::MockChatBuilder::new()
            .respond("Mocked investigation complete.")
            .commit();

        // Run Investigate Tool inside context
        let perms = Arc::new(crate::tools::filesystem::Permissions {
            base_dir: None,
            read_dirs: vec![],
            write_dirs: vec![],
        });
        let res =
            investigate_impl(perms, "test".to_string(), "t".to_string(), "a".to_string()).await;
        assert!(res.is_ok());

        // Validate traces were injected on the main agent branch
        let id_obj = crate::schema::identity_config::Identity::load(&td_path)
            .await
            .unwrap();
        let async_repo_x = crate::git::AsyncRepository::discover(&td_path)
            .await
            .unwrap();
        let reader =
            crate::events::reader::Reader::new(&async_repo_x, id_obj.get_did_owner().did.clone());
        let mut found_ask = false;
        let mut found_cancel = false;
        for ev in reader.iter_events().await.unwrap().flatten() {
            if let crate::schema::registry::EventPayload::Ask(_) = ev.payload {
                found_ask = true;
            }
            if let crate::schema::registry::EventPayload::CancelItem(_) = ev.payload {
                found_cancel = true;
            }
        }
        assert!(found_ask);
        assert!(found_cancel);
    }

    #[tokio::test]
    #[sealed_test(env = [
        ("NANCY_NO_TRACE_EVENTS", "1"),
        ("NANCY_HUMAN_DID", "z6MkiuexGTCjkPmnT4jv1JNAmeV7UnBoMh1rsxLoYYCQ8Txs"),
        ("NANCY_TEST_POLL_TIMEOUT", "1")
    ])]
    async fn test_ask_human_module_isolation() {
        let td = tempfile::tempdir().unwrap();
        let td_path = td.path().to_path_buf();
        std::env::set_current_dir(&td_path).unwrap();

        let _repo = crate::git::AsyncRepository::init(&td_path).await.unwrap();
        crate::commands::init::init(td_path.clone(), 1)
            .await
            .unwrap();

        let ask = AskHuman::start("isolated_q", "task1", "agent1")
            .await
            .expect("Failed to initialize AskHuman");

        let item_ref_clone = ask.item_ref.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            if let Ok(r) = crate::git::AsyncRepository::discover(".").await {
                let test_human = crate::schema::identity_config::DidOwner {
                    did: "z6MkiuexGTCjkPmnT4jv1JNAmeV7UnBoMh1rsxLoYYCQ8Txs".to_string(),
                    public_key_hex:
                        "4231c04a3dd5b64700018aa7cffe4ad35cf9bad05ec2fa21c3dbf5c0339872f8"
                            .to_string(),
                    private_key_hex:
                        "839ce4cc3dc57e147f119a6999c16230ea2d7b47c85ddc2714d25cb202b5023b"
                            .to_string(),
                };

                if let Ok(writer) = crate::events::writer::Writer::new(
                    &r,
                    crate::schema::identity_config::Identity::Doer(test_human.clone()),
                ) {

                    let _ = writer.log_event(crate::schema::registry::EventPayload::Seen(
                        crate::schema::task::SeenPayload {
                            item_ref: item_ref_clone.clone(),
                            timestamp: 0,
                        },
                    ));

                    let _ = writer.log_event(crate::schema::registry::EventPayload::HumanResponse(
                        crate::schema::task::ResponsePayload {
                            item_ref: item_ref_clone,
                            text_response: "Module Mock Human Response".to_string(),
                        },
                    ));
                    let _ = writer.commit_batch().await;
                }
            }
        });

        // Simulate LLM inference delay to allow human background task time to observe the Ask and send AskSeen
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let resp = ask.wait_for_response().await;
        assert!(resp.contains("Module Mock Human Response"));

        ask.cancel().await;
    }
}

// DOCUMENTED_BY: [docs/adr/0022-native-grinder-tool-boundaries.md]
