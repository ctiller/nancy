use leptos::prelude::*;

use crate::schema::{GrinderStatus, SerializedElement, SerializedFrame};

// Disk IO and get_active_grinders migrated cleanly to coordinator Axum backend explicitly natively structurally correctly elegantly flawlessly smoothly.#[component]
pub fn AgentsView() -> impl IntoView {
    let reload_trigger = Trigger::new();
    
    #[allow(unused_variables)]
    let (list, set_list) = signal::<Option<Vec<GrinderStatus>>>(None);
    
    #[cfg(feature = "hydrate")]
    {
        leptos::task::spawn_local(async move {
            let mut last_version: Option<u64> = None;
            loop {
                reload_trigger.track();
                let url = if let Some(lv) = last_version {
                    format!("/api/grinders?last_version={}", lv)
                } else {
                    "/api/grinders".to_string()
                };
                
                if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
                    if resp.status() == 200 {
                        if let Ok(text) = resp.text().await {
                            if let Ok(data) = serde_json::from_str::<crate::schema::GrindersResponse>(&text) {
                                last_version = Some(data.version);
                                set_list.set(Some(data.grinders));
                                continue;
                            }
                        }
                    }
                }
                
                gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    view! {
        <div class="glass-panel" style="padding: 20px;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
                <h2 style="margin: 0;">"Active Grinders"</h2>
                <button
                    class="btn"
                    style="background: rgba(0, 200, 255, 0.2); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 6px 12px; border-radius: 4px; cursor: pointer; font-family: monospace; font-size: 0.9rem;"
                    on:click=move |_| {
                        leptos::task::spawn_local(async move {
                            #[cfg(feature = "hydrate")]
                            {
                                let _ = gloo_net::http::Request::post("/api/add-grinder").send().await;
                            }
                            reload_trigger.notify();
                        });
                    }
                >
                    "+ Add Grinder"
                </button>
            </div>
            <Suspense fallback=move || view! { <div>"Loading grinders..."</div> }>
                <div style="display: flex; flex-direction: column; gap: 16px;">
                    {move || match list.get() {
                        None => leptos::either::Either::Left(view! { <div class="text-muted">"Loading..."</div> }),
                        Some(items) => {
                            if items.is_empty() {
                                leptos::either::Either::Left(view! { <div class="text-muted">"No active grinders found."</div> })
                            } else {
                                leptos::either::Either::Right(view! {
                                    <For
                                        each=move || items.clone()
                                        key=|status| format!("{}_{}_{}", status.did, status.is_online, status.failures.unwrap_or(0))
                                        children=move |status| view! { <AgentCard status=status.clone() reload_trigger=reload_trigger /> }
                                    />
                                })
                            }
                        }
                    }}
                </div>
            </Suspense>
        </div>
    }
}

#[component]
fn AgentCard(status: GrinderStatus, reload_trigger: Trigger) -> impl IntoView {
    #[allow(unused_variables)]
    let (state, set_state) = signal::<Option<SerializedFrame>>(None);
    #[allow(unused_variables)]
    let (is_online, set_is_online) = signal::<bool>(status.is_online);
    let (crash_log, set_crash_log) = signal::<Option<String>>(None);
    let did = status.did.clone();
    let log_ref = status.log_ref.clone();

    #[cfg(feature = "hydrate")]
    {
        if let Some(ref l_ref) = log_ref {
            let l_ref_clone = l_ref.clone();
            leptos::task::spawn_local(async move {
                if let Ok(res) = gloo_net::http::Request::get(&format!("/api/incidents/{}", l_ref_clone)).send().await {
                    if let Ok(text) = res.text().await {
                        set_crash_log.set(Some(text));
                    }
                }
            });
        }

        let did_clone = did.clone();
        leptos::task::spawn_local(async move {
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
            <div style="display:flex; align-items:center; justify-content: space-between; margin-bottom: 12px;">
                <div style="display:flex; align-items:center; gap:8px;">
                    <div class="status-dot" 
                        style=move || format!(
                            "background-color: {}; box-shadow: {}; animation: {};",
                            if is_online.get() { "var(--accent-cyan)" } else { "var(--text-muted)" },
                            if is_online.get() { "0 0 8px var(--accent-cyan)" } else { "none" },
                            if is_online.get() { "pulse 2s infinite" } else { "none" }
                        )></div>
                    <h3 style="margin: 0; font-family: monospace; text-transform: capitalize;">{status.agent_type.clone()}"::"
                        <span style=move || format!("color: {};", if is_online.get() { "var(--accent-cyan)" } else { "var(--text-muted)" })>{did.clone()}</span>
                    </h3>
                    <a href="/tasks" style="margin-left: 12px; font-size: 0.75rem; padding: 2px 6px; border-radius: 4px; border: 1px solid var(--accent-purple); color: var(--accent-purple); text-decoration: none;">"view map"</a>
                </div>
                <button
                    style="background: transparent; border: 1px solid var(--accent-red); color: var(--accent-red); padding: 4px 8px; border-radius: 4px; cursor: pointer; font-family: monospace; font-size: 0.8rem;"
                    on:click=move |_| {
                        set_is_online.set(false);
                        let did_move = did.clone();
                        leptos::task::spawn_local(async move {
                            #[cfg(feature = "hydrate")]
                            {
                                let did_clone = did_move;
                                let _ = gloo_net::http::Request::post("/api/remove-grinder")
                                    .header("Content-Type", "application/json")
                                    .json(&serde_json::json!({"did": did_clone}))
                                    .unwrap()
                                    .send().await;
                            }
                            reload_trigger.notify();
                        });
                    }
                >
                    "✖ Remove"
                </button>
            </div>
            
            <div style="padding: 12px; background: rgba(0, 0, 0, 0.4); border-radius: 8px; border: 1px solid var(--panel-border); font-family: monospace; font-size: 0.9rem; overflow-x: auto;">
                {
                    let log_ref_is_some = log_ref.is_some();
                    move || if !is_online.get() {
                    let text = if let (Some(failures), Some(next_unix)) = (status.failures.clone(), status.next_restart_at_unix.clone()) {
                        let now = js_sys::Date::now() / 1000.0;
                        let mut diff = (next_unix as f64 - now).round();
                        if diff < 0.0 { diff = 0.0; }
                        format!("Agent crashed ({} failures). Retrying in {}s...", failures, diff)
                    } else {
                        "Agent is currently offline...".to_string()
                    };
                    leptos::either::Either::Left(view! {
                        <div>
                            <div class="text-muted" style="margin-bottom: 8px;">{text}</div>
                            {move || if log_ref_is_some {
                                leptos::either::Either::Left(match crash_log.get() {
                                    Some(log) => leptos::either::Either::Left(view! {
                                        <div 
                                            style="background: rgba(255, 0, 0, 0.1); border-left: 2px solid var(--accent-red); padding: 8px 8px 24px 8px; border-radius: 0 4px 4px 0; white-space: pre; font-size: 0.8rem; overflow: auto; max-height: 400px; color: var(--text-muted); font-family: monospace;"
                                            prop:innerHTML={log}
                                        >
                                        </div>
                                    }),
                                    None => leptos::either::Either::Right(view! { <span class="text-muted" style="font-size: 0.8rem;">"Loading diagnostic log..."</span> })
                                })
                            } else {
                                leptos::either::Either::Right(view! { <span></span> })
                            }}
                        </div>
                    })
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

fn render_markdown(text: &str) -> String {
    use pulldown_cmark::{Parser, html};
    let parser = Parser::new(text);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

fn highlight_json(value: &serde_json::Value) -> String {
    let raw = serde_json::to_string_pretty(value).unwrap_or_default();
    
    let mut html = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
             let mut start = i;
             i += 1;
             while i < chars.len() {
                 if chars[i] == '"' && chars[i-1] != '\\' {
                     i += 1;
                     break;
                 }
                 i += 1;
             }
             let content = &raw[start..i];
             
             let mut is_key = false;
             let mut j = i;
             while j < chars.len() && chars[j].is_whitespace() { j += 1; }
             if j < chars.len() && chars[j] == ':' {
                 is_key = true;
             }
             
             let escaped = content.replace("<", "&lt;").replace(">", "&gt;");
             if is_key {
                 html.push_str(&format!("<span class=\"ts-property\">{}</span>", escaped));
             } else {
                 html.push_str(&format!("<span class=\"ts-string\">{}</span>", escaped));
             }
        } else if ch.is_ascii_digit() || ch == '-' {
             let start = i;
             while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == 'e' || chars[i] == 'E' || chars[i] == '-' || chars[i] == '+') {
                 i += 1;
             }
             html.push_str(&format!("<span class=\"ts-constant\">{}</span>", &raw[start..i]));
        } else if raw[i..].starts_with("true") || raw[i..].starts_with("false") || raw[i..].starts_with("null") {
             if raw[i..].starts_with("true") { html.push_str("<span class=\"ts-keyword\">true</span>"); i += 4; }
             else if raw[i..].starts_with("false") { html.push_str("<span class=\"ts-keyword\">false</span>"); i += 5; }
             else { html.push_str("<span class=\"ts-keyword\">null</span>"); i += 4; }
        } else if ch == '{' || ch == '}' || ch == '[' || ch == ']' || ch == ',' || ch == ':' {
             html.push_str(&format!("<span class=\"ts-punctuation\">{}</span>", ch));
             i += 1;
        } else {
             if ch == '<' { html.push_str("&lt;"); }
             else if ch == '>' { html.push_str("&gt;"); }
             else { html.push_str(&ch.to_string()); }
             i += 1;
        }
    }
    
    html
}

#[component]
fn FrameView(frame: SerializedFrame) -> impl IntoView {
    view! {
        <details class="agent-frame custom-details" open>
            <summary class="frame-name" style="cursor: pointer; display: flex; justify-content: flex-start; align-items: center; gap: 8px;">
                <span class="chevron"></span>
                <div>
                    "[" {frame.name} "]"
                    {
                        if let Some(st) = frame.status.clone() {
                            view! { <span class="sparkle-cursor" style="margin-left: 8px;">{st}</span> }.into_any()
                        } else {
                            view! { <span style="display: none;"></span> }.into_any()
                        }
                    }
                </div>
            </summary>
            <div class="frame-body">
                <For
                    each=move || frame.elements.clone().into_iter().enumerate()
                    key=|(i, _)| *i
                    children=move |(_i, el)| {
                        match el {
                            SerializedElement::Log { message } => leptos::either::Either::Left(leptos::either::Either::Left(view! {
                                <div class="agent-element log-element">
                                    <span class="log-arrow">">"</span> " " {message}
                                </div>
                            })),
                            SerializedElement::Data { key, value } => {
                                if key == "system_prompt" || key == "user_prompt" {
                                    let md_html = render_markdown(value.as_str().unwrap_or(""));
                                    leptos::either::Either::Left(leptos::either::Either::Right(view! {
                                        <details class="agent-element data-element custom-details" style="margin-bottom: 12px; margin-top: 8px;">
                                            <summary style="display: flex; justify-content: flex-start; align-items: center; gap: 8px; cursor: pointer; opacity: 0.8; margin-bottom: 4px; padding: 4px;">
                                                <span class="chevron"></span>
                                                <div class="data-key" style="margin-bottom: 0;">{key}":"</div>
                                            </summary>
                                            <div class="data-val" style="padding: 12px; background: rgba(0,0,0,0.3); border-radius: 6px; border-left: 2px solid var(--accent-purple);" prop:innerHTML={md_html}></div>
                                        </details>
                                    }))
                                } else {
                                    let json_html = highlight_json(&value);
                                    leptos::either::Either::Right(leptos::either::Either::Left(view! {
                                        <details class="agent-element data-element custom-details" style="margin-bottom: 8px; margin-top: 8px;">
                                            <summary style="display: flex; justify-content: flex-start; align-items: center; gap: 8px; cursor: pointer; opacity: 0.8; margin-bottom: 4px; padding: 4px;">
                                                <span class="chevron"></span>
                                                <div class="data-key" style="margin-bottom: 0;">{key}": "</div>
                                            </summary>
                                            <pre class="data-val tree-sitter-wrapper" style="margin-top: 4px; padding: 12px; background: rgba(0,0,0,0.4); border-radius: 6px;" prop:innerHTML={json_html}></pre>
                                        </details>
                                    }))
                                }
                            },
                            SerializedElement::Frame(child_frame) => leptos::either::Either::Right(leptos::either::Either::Right(view! {
                                <FrameView frame=child_frame />
                            }.into_any()))
                        }
                    }
                />
            </div>
        </details>
    }
}

// Test bounds migrated appropriately natively.
