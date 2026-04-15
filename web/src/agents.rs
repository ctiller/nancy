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

use schema::{GrinderStatus, GrindersResponse, SerializedElement, SerializedFrame};
use yew::prelude::*;

#[function_component(AgentsView)]
pub fn agents_view() -> Html {
    let list = use_state(|| None::<Vec<GrinderStatus>>);
    let reload_trigger = use_state(|| 0);

    {
        let list = list.clone();
        let reload = *reload_trigger;
        use_effect_with(reload, move |_| {
            let mut last_version: Option<u64> = None;
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            let abort_controller = web_sys::AbortController::new().ok();
            let signal = abort_controller.as_ref().map(|ac| ac.signal());

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    if cancel_clone.get() {
                        break;
                    }
                    let url = if let Some(lv) = last_version {
                        format!("/api/grinders?last_version={}", lv)
                    } else {
                        "/api/grinders".to_string()
                    };

                    let mut req = gloo_net::http::Request::get(&url);
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }

                    if let Ok(resp) = req.send().await {
                        if cancel_clone.get() {
                            break;
                        }
                        if resp.ok() {
                            let parse_res = resp.json::<GrindersResponse>().await;
                            if let Ok(data) = parse_res {
                                if Some(data.version) != last_version {
                                    last_version = Some(data.version);
                                    list.set(Some(data.grinders));
                                }
                            } else if let Err(e) = parse_res {
                                web_sys::console::error_1(
                                    &format!("Failed to parse GrindersResponse: {:?}", e).into(),
                                );
                            }
                        } else {
                            web_sys::console::error_1(
                                &format!("Agents fetch failed with status: {}", resp.status())
                                    .into(),
                            );
                        }
                    } else {
                        web_sys::console::error_1(&"Network error in Agents fetch".into());
                    }
                    if cancel_clone.get() {
                        break;
                    }
                    gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                }
            });

            move || {
                cancelled.set(true);
                if let Some(ac) = abort_controller {
                    ac.abort();
                }
            }
        });
    }

    let on_add_grinder = {
        let reload_trigger = reload_trigger.clone();
        Callback::from(move |_| {
            let reload_trigger = reload_trigger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = gloo_net::http::Request::post("/api/add-grinder")
                    .send()
                    .await;
                reload_trigger.set(*reload_trigger + 1);
            });
        })
    };

    html! {
        <div class="glass-panel" style="padding: 20px;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;">
                <h2 style="margin: 0;">{"Active Grinders"}</h2>
                <button
                    class="btn"
                    style="background: rgba(0, 200, 255, 0.2); border: 1px solid var(--accent-cyan); color: var(--accent-cyan); padding: 6px 12px; border-radius: 4px; cursor: pointer; font-family: monospace; font-size: 0.9rem;"
                    onclick={on_add_grinder}
                >
                    {"+ Add Grinder"}
                </button>
            </div>
            <div style="display: flex; flex-direction: column; gap: 16px;">
                if let Some(items) = &*list {
                    if items.is_empty() {
                        <div class="text-muted">{"No active grinders found."}</div>
                    } else {
                        { for items.iter().map(|status| {
                            let key = format!("{}_{}_{}", status.did, status.is_online, status.failures.unwrap_or(0));
                            html! { <AgentCard key={key} status={status.clone()} reload_trigger={reload_trigger.clone()} /> }
                        })}
                    }
                } else {
                    <div class="text-muted">{"Loading..."}</div>
                }
            </div>
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct AgentCardProps {
    status: GrinderStatus,
    reload_trigger: UseStateHandle<u32>,
}

#[function_component(AgentCard)]
fn agent_card(props: &AgentCardProps) -> Html {
    let state = use_state(|| None::<SerializedFrame>);
    let is_online = use_state(|| props.status.is_online);
    let crash_log = use_state(|| None::<String>);

    let did = props.status.did.clone();
    let log_ref = props.status.log_ref.clone();

    {
        let did = did.clone();
        let log_ref = log_ref.clone();
        let state = state.clone();
        let is_online = is_online.clone();
        let crash_log = crash_log.clone();

        use_effect_with((), move |_| {
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            let abort_controller = web_sys::AbortController::new().ok();
            let signal = abort_controller.as_ref().map(|ac| ac.signal());

            if let Some(l_ref) = log_ref {
                let crash_log = crash_log.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Ok(res) =
                        gloo_net::http::Request::get(&format!("/api/incidents/{}", l_ref))
                            .send()
                            .await
                    {
                        if let Ok(text) = res.text().await {
                            crash_log.set(Some(text));
                        }
                    }
                });
            }

            wasm_bindgen_futures::spawn_local(async move {
                let mut last_update: Option<u64> = None;
                loop {
                    if cancel_clone.get() {
                        break;
                    }
                    let url = if let Some(lu) = last_update {
                        format!("/api/grinders/{}/state?last_update={}", did, lu)
                    } else {
                        format!("/api/grinders/{}/state", did)
                    };

                    let mut req = gloo_net::http::Request::get(&url);
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }

                    if let Ok(resp) = req.send().await {
                        if cancel_clone.get() {
                            break;
                        }
                        if resp.ok() {
                            if let Ok(json) = resp.json::<serde_json::Value>().await {
                                if let (Some(new_update), Some(frame_val)) = (
                                    json.get("update_number").and_then(|v| v.as_u64()),
                                    json.get("tree"),
                                ) {
                                    if Some(new_update) != last_update {
                                        if let Ok(frame) = serde_json::from_value::<SerializedFrame>(
                                            frame_val.clone(),
                                        ) {
                                            last_update = Some(new_update);
                                            state.set(Some(frame));
                                            is_online.set(true);
                                        }
                                    } else {
                                        is_online.set(true);
                                    }
                                } else {
                                    web_sys::console::error_1(
                                        &"AgentCard missing update_number or tree in payload"
                                            .into(),
                                    );
                                    is_online.set(true);
                                }
                            } else if let Err(e) = resp.json::<serde_json::Value>().await {
                                web_sys::console::error_1(
                                    &format!("AgentCard failed to parse JSON: {:?}", e).into(),
                                );
                            }
                        } else {
                            web_sys::console::error_1(
                                &format!("AgentCard fetch failed with status: {}", resp.status())
                                    .into(),
                            );
                            is_online.set(false);
                        }
                    } else {
                        web_sys::console::error_1(&"Network error in AgentCard fetch".into());
                        is_online.set(false);
                    }
                    if cancel_clone.get() {
                        break;
                    }
                    gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                }
            });

            move || {
                cancelled.set(true);
                if let Some(ac) = abort_controller {
                    ac.abort();
                }
            }
        });
    }

    let on_remove = {
        let is_online = is_online.clone();
        let did = did.clone();
        let reload_trigger = props.reload_trigger.clone();
        Callback::from(move |_| {
            is_online.set(false);
            let did = did.clone();
            let reload_trigger = reload_trigger.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = gloo_net::http::Request::post("/api/remove-grinder")
                    .json(&serde_json::json!({"did": did}))
                    .unwrap()
                    .send()
                    .await;
                reload_trigger.set(*reload_trigger + 1);
            });
        })
    };

    let online_val = *is_online;
    let style = format!(
        "padding: 16px; margin-bottom: 12px; background: rgba(0,0,0,0.2); border-left: {}; opacity: {};",
        if online_val {
            "4px solid var(--accent-cyan)"
        } else {
            "4px solid var(--text-muted)"
        },
        if online_val { "1.0" } else { "0.6" }
    );

    let is_active = online_val
        && state
            .as_ref()
            .map(|f| f.status.as_deref() != Some("Waiting for assignments..."))
            .unwrap_or(false);

    let dot_style = format!(
        "background-color: {}; box-shadow: {}; animation: {};",
        if online_val {
            "var(--accent-cyan)"
        } else {
            "var(--text-muted)"
        },
        if online_val {
            "0 0 8px var(--accent-cyan)"
        } else {
            "none"
        },
        if is_active {
            "pulse 2s infinite"
        } else {
            "none"
        }
    );

    html! {
        <div class="glass-panel agent-card" style={style}>
            <div style="display:flex; align-items:center; justify-content: space-between; margin-bottom: 12px;">
                <div style="display:flex; align-items:center; gap:8px;">
                    <div class="status-dot" style={dot_style}></div>
                    <h3 style="margin: 0; font-family: monospace; text-transform: capitalize;">
                        {props.status.agent_type.clone()}{"::"}
                        <span style={format!("color: {};", if online_val { "var(--accent-cyan)" } else { "var(--text-muted)" })}>{did.clone()}</span>
                    </h3>
                    <a href="/tasks" style="margin-left: 12px; font-size: 0.75rem; padding: 2px 6px; border-radius: 4px; border: 1px solid var(--accent-purple); color: var(--accent-purple); text-decoration: none;">{"view map"}</a>
                </div>
                <button
                    style="background: transparent; border: 1px solid var(--accent-red); color: var(--accent-red); padding: 4px 8px; border-radius: 4px; cursor: pointer; font-family: monospace; font-size: 0.8rem;"
                    onclick={on_remove}
                >
                    {"✖ Remove"}
                </button>
            </div>

            <div style="padding: 12px; background: rgba(0, 0, 0, 0.4); border-radius: 8px; border: 1px solid var(--panel-border); font-family: monospace; font-size: 0.9rem; overflow-x: auto;">
                if !online_val {
                    <div>
                        <div class="text-muted" style="margin-bottom: 8px;">
                            {
                                if let (Some(failures), Some(next_unix)) = (props.status.failures, props.status.next_restart_at_unix) {
                                    let now = js_sys::Date::now() / 1000.0;
                                    let mut diff = (next_unix as f64 - now).round();
                                    if diff < 0.0 { diff = 0.0; }
                                    format!("Agent crashed ({} failures). Retrying in {}s...", failures, diff)
                                } else {
                                    "Agent is currently offline...".to_string()
                                }
                            }
                        </div>
                        if props.status.log_ref.is_some() {
                            if let Some(log) = &*crash_log {
                                <div
                                    style="background: rgba(255, 0, 0, 0.1); border-left: 2px solid var(--accent-red); padding: 8px 8px 24px 8px; border-radius: 0 4px 4px 0; white-space: pre; font-size: 0.8rem; overflow: auto; max-height: 400px; color: var(--text-muted); font-family: monospace;"
                                >
                                    {Html::from_html_unchecked(AttrValue::from(log.clone()))}
                                </div>
                            } else {
                                <span class="text-muted" style="font-size: 0.8rem;">{"Loading diagnostic log..."}</span>
                            }
                        }
                    </div>
                } else if let Some(frame) = &*state {
                    if let Some(ref rollup_text) = frame.rollup {
                        <div style="margin-bottom: 12px; padding: 12px; background: rgba(0, 200, 255, 0.05); border-left: 3px solid var(--accent-cyan); border-radius: 4px; color: var(--text-color); font-family: sans-serif; font-size: 0.95rem; line-height: 1.5; font-style: italic;">
                            <span style="font-weight: bold; color: var(--accent-cyan); font-style: normal; margin-right: 6px;">{"Summary:"}</span>
                            {rollup_text.clone()}
                        </div>
                    }
                    <FrameView frame={frame.clone()} />
                } else {
                    <div class="text-muted">{"Waiting for state..."}</div>
                }
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
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' && chars[i - 1] != '\\' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let content = &raw[start..i];

            let mut is_key = false;
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
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
            while i < chars.len()
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'e'
                    || chars[i] == 'E'
                    || chars[i] == '-'
                    || chars[i] == '+')
            {
                i += 1;
            }
            html.push_str(&format!(
                "<span class=\"ts-constant\">{}</span>",
                &raw[start..i]
            ));
        } else if raw[i..].starts_with("true")
            || raw[i..].starts_with("false")
            || raw[i..].starts_with("null")
        {
            if raw[i..].starts_with("true") {
                html.push_str("<span class=\"ts-keyword\">true</span>");
                i += 4;
            } else if raw[i..].starts_with("false") {
                html.push_str("<span class=\"ts-keyword\">false</span>");
                i += 5;
            } else {
                html.push_str("<span class=\"ts-keyword\">null</span>");
                i += 4;
            }
        } else if ch == '{' || ch == '}' || ch == '[' || ch == ']' || ch == ',' || ch == ':' {
            html.push_str(&format!("<span class=\"ts-punctuation\">{}</span>", ch));
            i += 1;
        } else {
            if ch == '<' {
                html.push_str("&lt;");
            } else if ch == '>' {
                html.push_str("&gt;");
            } else {
                html.push_str(&ch.to_string());
            }
            i += 1;
        }
    }

    html
}

#[derive(Properties, PartialEq)]
struct FrameViewProps {
    frame: SerializedFrame,
}

#[function_component(FrameView)]
fn frame_view(props: &FrameViewProps) -> Html {
    let frame = &props.frame;
    html! {
        <details class="agent-frame custom-details" open=true>
            <summary class="frame-name" style="cursor: pointer; display: flex; justify-content: flex-start; align-items: center; gap: 8px;">
                <span class="chevron"></span>
                <div>
                    {"["} {&frame.name} {"]"}
                    if let Some(ref st) = frame.status {
                        <span class="sparkle-cursor" style="margin-left: 8px;">{st}</span>
                    }
                </div>
            </summary>
            <div class="frame-body">
                { for frame.elements.iter().enumerate().map(|(_i, el)| {
                    match el {
                        SerializedElement::Log { message } => html! {
                            <div class="agent-element log-element">
                                <span class="log-arrow">{">"}</span> {" "} {message}
                            </div>
                        },
                        SerializedElement::Data { key, value } => {
                            if key == "system_prompt" || key == "user_prompt" || key == "response" {
                                let md_html = render_markdown(value.as_str().unwrap_or(""));
                                html! {
                                    <details class="agent-element data-element custom-details" style="margin-bottom: 12px; margin-top: 8px;">
                                        <summary style="display: flex; justify-content: flex-start; align-items: center; gap: 8px; cursor: pointer; opacity: 0.8; margin-bottom: 4px; padding: 4px;">
                                            <span class="chevron"></span>
                                            <div class="data-key" style="margin-bottom: 0;">{key}{":"}</div>
                                        </summary>
                                        <div class="data-val" style="padding: 12px; background: rgba(0,0,0,0.3); border-radius: 6px; border-left: 2px solid var(--accent-purple);">
                                            {Html::from_html_unchecked(AttrValue::from(md_html))}
                                        </div>
                                    </details>
                                }
                            } else {
                                let json_html = highlight_json(value);
                                html! {
                                    <details class="agent-element data-element custom-details" style="margin-bottom: 8px; margin-top: 8px;">
                                        <summary style="display: flex; justify-content: flex-start; align-items: center; gap: 8px; cursor: pointer; opacity: 0.8; margin-bottom: 4px; padding: 4px;">
                                            <span class="chevron"></span>
                                            <div class="data-key" style="margin-bottom: 0;">{key}{": "}</div>
                                        </summary>
                                        <pre class="data-val tree-sitter-wrapper" style="margin-top: 4px; padding: 12px; background: rgba(0,0,0,0.4); border-radius: 6px;">
                                            {Html::from_html_unchecked(AttrValue::from(json_html))}
                                        </pre>
                                    </details>
                                }
                            }
                        },
                        SerializedElement::Frame(child_frame) => html! {
                            <FrameView frame={child_frame.clone()} />
                        }
                    }
                })}
            </div>
        </details>
    }
}
