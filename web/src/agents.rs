use leptos::prelude::*;
#[cfg(feature = "hydrate")]
use leptos::task::spawn_local;

use crate::schema::{GrinderStatus, SerializedElement, SerializedFrame};

#[server(GetActiveGrinders, "/api")]
pub async fn get_active_grinders() -> Result<Vec<GrinderStatus>, ServerFnError> {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let nancy_dir = root.join(".nancy");
    
    let mut statuses = vec![];
    let identity_path = nancy_dir.join("identity.json");
    if let Ok(data) = tokio::fs::read_to_string(&identity_path).await {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) {
            if json.get("type").and_then(|t| t.as_str()) == Some("Coordinator") {
                if let Some(workers) = json.get("workers").and_then(|w| w.as_array()) {
                    for worker in workers {
                        if let Some(did) = worker.get("did").and_then(|d| d.as_str()) {
                            statuses.push(GrinderStatus {
                                did: did.to_string(),
                                is_online: false,
                            });
                        }
                    }
                }
            }
        }
    }

    let sockets_dir = nancy_dir.join("sockets");
    if let Ok(mut entries) = tokio::fs::read_dir(&sockets_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let meta = entry.metadata().await;
            if meta.map(|m| m.is_dir()).unwrap_or(false) {
                let did = entry.file_name().to_string_lossy().to_string();
                if did != "coordinator" && tokio::fs::metadata(entry.path().join("grinder.sock")).await.is_ok() {
                    if let Some(existing) = statuses.iter_mut().find(|s| s.did == did) {
                        existing.is_online = true;
                    } else {
                        statuses.push(GrinderStatus {
                            did: did.to_string(),
                            is_online: true,
                        });
                    }
                }
            }
        }
    }
    Ok(statuses)
}

#[component]
pub fn AgentsView() -> impl IntoView {
    let grinders = Resource::new(|| (), |_| async move {
        get_active_grinders().await.unwrap_or_default()
    });

    view! {
        <div class="glass-panel" style="padding: 20px;">
            <h2>"Active Grinders"</h2>
            <Suspense fallback=move || view! { <div>"Loading grinders..."</div> }>
                <div style="display: flex; flex-direction: column; gap: 16px;">
                    {move || grinders.get().map(|list| {
                        if list.is_empty() {
                            leptos::either::Either::Left(view! { <div class="text-muted">"No active grinders found."</div> })
                        } else {
                            leptos::either::Either::Right(view! {
                                <For
                                    each=move || list.clone()
                                    key=|status| status.did.clone()
                                    children=move |status| view! { <AgentCard status=status.clone() /> }
                                />
                            })
                        }
                    })}
                </div>
            </Suspense>
        </div>
    }
}

#[component]
fn AgentCard(status: GrinderStatus) -> impl IntoView {
    #[allow(unused_variables)]
    let (state, set_state) = signal::<Option<SerializedFrame>>(None);
    #[allow(unused_variables)]
    let (is_online, set_is_online) = signal::<bool>(status.is_online);
    let did = status.did.clone();

    #[cfg(feature = "hydrate")]
    {
        let did_clone = did.clone();
        spawn_local(async move {
            // Delay to wait for full SSR hydration to mount to avoid hydration panics on instant error returns.
            gloo_timers::future::sleep(std::time::Duration::from_millis(1500)).await;
            let mut last_update: Option<u64> = None;
            loop {
                let url = if let Some(lu) = last_update {
                    format!("/api/grinders/{}/state?last_update={}", did_clone, lu)
                } else {
                    format!("/api/grinders/{}/state", did_clone)
                };
                if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
                    if resp.status() == 200 {
                        if let Ok(text) = resp.text().await {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                if let (Some(new_update), Some(frame_val)) = (
                                    json.get("update_number").and_then(|v| v.as_u64()),
                                    json.get("tree")
                                ) {
                                    if let Ok(frame) = serde_json::from_value::<SerializedFrame>(frame_val.clone()) {
                                        last_update = Some(new_update);
                                        set_state.set(Some(frame));
                                        set_is_online.set(true); // Agent successfully synced actively

                                        // The backend correctly long-polls 30s. We instantly loop back to block on the next long-poll.
                                        continue;
                                    }
                                }
                            }
                        }
                    } else {
                        set_is_online.set(false); // Gracefully downgrade explicitly due to error mapping natively (404/500)
                    }
                } else {
                    set_is_online.set(false);
                }
                
                // Delay gracefully only upon structural or protocol errors
                gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    view! {
        <div class="glass-panel agent-card"
            style=move || format!(
                "padding: 16px; margin-bottom: 12px; background: rgba(0,0,0,0.2); border-left: {}; opacity: {};",
                if is_online.get() { "4px solid var(--accent-cyan)" } else { "4px solid var(--text-muted)" },
                if is_online.get() { "1.0" } else { "0.6" }
            )>
            <div style="display:flex; align-items:center; gap:8px; margin-bottom: 12px;">
                <div class="status-dot" 
                    style=move || format!(
                        "background-color: {}; box-shadow: {}; animation: {};",
                        if is_online.get() { "var(--accent-cyan)" } else { "var(--text-muted)" },
                        if is_online.get() { "0 0 8px var(--accent-cyan)" } else { "none" },
                        if is_online.get() { "pulse 2s infinite" } else { "none" }
                    )></div>
                <h3 style="margin: 0; font-family: monospace;">"Grinder::"
                    <span style=move || format!("color: {};", if is_online.get() { "var(--accent-cyan)" } else { "var(--text-muted)" })>{did.clone()}</span>
                </h3>
            </div>
            
            <div style="padding: 12px; background: rgba(0, 0, 0, 0.4); border-radius: 8px; border: 1px solid var(--panel-border); font-family: monospace; font-size: 0.9rem; overflow-x: auto;">
                {move || if !is_online.get() {
                    leptos::either::Either::Left(view! { <div class="text-muted">"Agent is currently offline..."</div> })
                } else {
                    leptos::either::Either::Right(match state.get() {
                        Some(frame) => leptos::either::Either::Left(view! { <FrameView frame=frame /> }),
                        None => leptos::either::Either::Right(view! { <div class="text-muted">"Waiting for state..."</div> })
                    })
                }}
            </div>
        </div>
    }
}

#[component]
fn FrameView(frame: SerializedFrame) -> impl IntoView {
    view! {
        <div class="agent-frame">
            <div class="frame-name">
                "[" {frame.name} "]"
            </div>
            <div class="frame-body">
                <For
                    each=move || frame.elements.clone().into_iter().enumerate()
                    key=|(i, _)| *i
                    children=move |(_i, el)| {
                        match el {
                            SerializedElement::Log { message } => leptos::either::Either::Left(view! {
                                <div class="agent-element log-element">
                                    <span class="log-arrow">">"</span> " " {message}
                                </div>
                            }),
                            SerializedElement::Data { key, value } => leptos::either::Either::Right(leptos::either::Either::Left(view! {
                                <div class="agent-element data-element">
                                    <span class="data-key">{key}": "</span>
                                    <pre class="data-val">{serde_json::to_string_pretty(&value).unwrap_or_default()}</pre>
                                </div>
                            })),
                            SerializedElement::Frame(child_frame) => leptos::either::Either::Right(leptos::either::Either::Right(view! {
                                <FrameView frame=child_frame />
                            }.into_any()))
                        }
                    }
                />
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sealed_test::prelude::*;
    use tokio::fs::{self, File};

    #[sealed_test]
    fn test_get_active_grinders_parses_identity() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let tmp = tempfile::tempdir().unwrap();
            std::env::set_current_dir(tmp.path()).unwrap();
            
            let nancy_dir = tmp.path().join(".nancy");
            fs::create_dir_all(&nancy_dir).await.unwrap();
            
            // 1. Initial empty state
            let statuses = get_active_grinders().await.unwrap();
            assert!(statuses.is_empty(), "Empty environment should have no grinders");

            // 2. Add identity.json
            let identity_json = serde_json::json!({
                "type": "Coordinator",
                "did": { "did": "z6_coord_123" },
                "workers": [
                    { "did": "z6_worker_1" },
                    { "did": "z6_worker_2" }
                ]
            });
            fs::write(nancy_dir.join("identity.json"), identity_json.to_string()).await.unwrap();

            let statuses = get_active_grinders().await.unwrap();
            assert_eq!(statuses.len(), 2, "Should parse two workers from identity.json");
            
            // Ensure both are offline initially
            assert!(!statuses.iter().find(|s| s.did == "z6_worker_1").unwrap().is_online);
            assert!(!statuses.iter().find(|s| s.did == "z6_worker_2").unwrap().is_online);

            // 3. Add an active socket for worker 1
            let w1_dir = nancy_dir.join("sockets").join("z6_worker_1");
            fs::create_dir_all(&w1_dir).await.unwrap();
            File::create(w1_dir.join("grinder.sock")).await.unwrap();
            
            // Add an unknown/ad-hoc grinder socket
            let adhoc_dir = nancy_dir.join("sockets").join("z6_adhoc");
            fs::create_dir_all(&adhoc_dir).await.unwrap();
            File::create(adhoc_dir.join("grinder.sock")).await.unwrap();

            let statuses = get_active_grinders().await.unwrap();
            assert_eq!(statuses.len(), 3, "Should have 2 known workers + 1 ad-hoc worker");
            
            let w1 = statuses.iter().find(|s| s.did == "z6_worker_1").unwrap();
            assert!(w1.is_online, "Worker 1 should be marked online");

            let w2 = statuses.iter().find(|s| s.did == "z6_worker_2").unwrap();
            assert!(!w2.is_online, "Worker 2 should remain offline");
            
            let adhoc = statuses.iter().find(|s| s.did == "z6_adhoc").unwrap();
            assert!(adhoc.is_online, "Ad-hoc worker should be online dynamically");
        });
    }
}
