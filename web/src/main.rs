use yew::prelude::*;
use yew_router::prelude::*;
use wasm_bindgen::prelude::*;

pub mod repo;
pub mod agents;
pub mod tasks;

#[derive(Clone, Routable, PartialEq)]
enum Route {
    #[at("/")]
    Command,
    #[at("/tasks")]
    Tasks,
    #[at("/agents")]
    Agents,
    #[at("/repo")]
    Repo,
    #[at("/logs")]
    Logs,
    #[not_found]
    #[at("/404")]
    NotFound,
}

#[function_component(Navbar)]
fn navbar() -> Html {
    html! {
        <nav class="glass-nav">
            <div class="nav-brand">
                <img src="/nancy-avatar.png" alt="Nancy Logo" class="brand-logo" />
                <span>{"NANCY"}</span>
            </div>
            
            <div class="nav-links">
                <Link<Route> to={Route::Command} classes="nav-item">{"Command"}</Link<Route>>
                <Link<Route> to={Route::Tasks} classes="nav-item">{"Tasks"}</Link<Route>>
                <Link<Route> to={Route::Agents} classes="nav-item">{"Agents"}</Link<Route>>
                <Link<Route> to={Route::Repo} classes="nav-item">{"Repo"}</Link<Route>>
                <Link<Route> to={Route::Logs} classes="nav-item">{"Settings & Logs"}</Link<Route>>
            </div>
            
            <div class="status-indicator">
                <div class="status-dot"></div>
                <span>{"Coordinator Active"}</span>
            </div>
        </nav>
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window)]
    fn mountMonaco(id: &str);
    #[wasm_bindgen(js_namespace = window)]
    fn getMonacoValue() -> String;
}

#[function_component(CommandView)]
fn command_view() -> Html {
    let evals = use_state(|| vec![]);
    let is_creating_task = use_state(|| false);
    
    {
        let evals = evals.clone();
        use_effect_with((), move |_| {
            let mut last_version: Option<u64> = None;
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            let abort_controller = web_sys::AbortController::new().ok();
            let signal = abort_controller.as_ref().map(|ac| ac.signal());

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    if cancel_clone.get() { break; }
                    let url = if let Some(lv) = last_version {
                        format!("/api/tasks/evaluations?last_version={}", lv)
                    } else {
                        "/api/tasks/evaluations".to_string()
                    };

                    let mut req = gloo_net::http::Request::get(&url);
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }

                    if let Ok(resp) = req.send().await {
                        if cancel_clone.get() { break; }
                        if resp.ok() {
                            if let Ok(data) = resp.json::<serde_json::Value>().await {
                                if let (Some(ver), Some(eval_array)) = (
                                    data.get("version").and_then(|v| v.as_u64()),
                                    data.get("evaluations")
                                ) {
                                    if Some(ver) != last_version {
                                        if let Ok(parsed_evals) = serde_json::from_value::<Vec<schema::TaskEvaluation>>(eval_array.clone()) {
                                            last_version = Some(ver);
                                            evals.set(parsed_evals);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if cancel_clone.get() { break; }
                    gloo_timers::future::sleep(std::time::Duration::from_millis(500)).await;
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

    {
        let is_creating = *is_creating_task;
        use_effect_with(is_creating, move |&creating| {
            if creating {
                mountMonaco("monaco-container");
            }
            || ()
        });
    }

    let on_new_task = {
        let is_creating_task = is_creating_task.clone();
        Callback::from(move |_| {
            is_creating_task.set(true);
        })
    };

    let on_submit_task = {
        let is_creating_task = is_creating_task.clone();
        Callback::from(move |_| {
            let task_desc = getMonacoValue();
            let is_creating_task = is_creating_task.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let payload = schema::TaskRequestPayload {
                    requestor: "Admin Web UI".to_string(),
                    description: task_desc,
                };
                if let Ok(resp) = gloo_net::http::Request::post("/api/tasks")
                    .json(&payload)
                    .unwrap()
                    .send()
                    .await {
                    if resp.ok() {
                        is_creating_task.set(false);
                    }
                }
            });
        })
    };

    html! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px; position: relative; overflow-y: auto; display: flex; flex-direction: column;">
                <h3>{"Evaluated Agent Events"}</h3>
                <div style="flex: 1; overflow-y: auto;">
                    if evals.is_empty() {
                        <p class="text-muted">{"Waiting for Dreamer evaluation events..."}</p>
                    } else {
                        <div style="display: flex; flex-direction: column; gap: 12px; margin-bottom: 60px;">
                            { for evals.iter().map(|eval| {
                                let intensity = eval.score as f64 / 100.0;
                                let r = (255.0 * intensity) as u8;
                                let g = (255.0 * (1.0 - intensity).max(0.4)) as u8;
                                let color_style = format!("color: rgb({}, {}, 100); border-left: 4px solid rgb({}, {}, 100);", r, g, r, g);
                                html! {
                                    <div style={format!("padding: 12px; background: rgba(0,0,0,0.3); border-radius: 4px; {}", color_style)}>
                                        <div style="display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 4px;">
                                            <span style="font-weight: bold; font-family: monospace;">{ eval.event_type.clone() }</span>
                                            <span style="font-size: 1.2rem; font-weight: bold;">{eval.score}{"/100"}</span>
                                        </div>
                                        <div style="font-size: 0.8rem; color: var(--text-muted); font-family: monospace;">
                                            {"ID: "} {eval.id.clone()}
                                        </div>
                                    </div>
                                }
                            })}
                        </div>
                    }
                </div>
            </div>
            
            <div class="glass-panel" style="padding: 20px; display: flex; flex-direction: column; min-width: 0;">
                <h3>{"Workspace Editor"}</h3>
                if *is_creating_task {
                    <div class="task-editor" style="flex: 1; display: flex; flex-direction: column; gap: 12px; height: 100%;">
                        <div id="monaco-container" style="flex: 1; width: 100%; min-height: 400px; border-radius: 8px; overflow: hidden; border: 1px solid var(--panel-border); box-shadow: inset 0 0 10px rgba(0,0,0,0.5);"></div>
                        <div style="display: flex; justify-content: flex-end; gap: 12px; margin-top: 12px;">
                            <button class="btn-secondary" onclick={
                                let is_creating_task = is_creating_task.clone();
                                Callback::from(move |_| is_creating_task.set(false))
                            }>{"Cancel"}</button>
                            <button class="btn-primary" onclick={on_submit_task}>{"Submit Task"}</button>
                        </div>
                    </div>
                } else {
                    <p style="color: var(--text-muted); font-size: 1.1rem; flex: 1;">{"Select an inquiry or start a new task."}</p>
                    <button class="btn-glow" onclick={on_new_task}>{"+ New Task"}</button>
                }
            </div>
        </div>
    }
}

fn switch(routes: Route) -> Html {
    match routes {
        Route::Command => html! { <CommandView /> },
        Route::Tasks => html! { <tasks::TasksView /> },
        Route::Agents => html! { <agents::AgentsView /> },
        Route::Repo => html! { <repo::RepoView /> },
        Route::Logs => html! { <div>{"Logs"}</div> },
        Route::NotFound => html! { <h1>{ "404 - Not Found" }</h1> },
    }
}

#[function_component(App)]
pub fn app() -> Html {
    html! {
        <BrowserRouter>
            <Navbar />
            <main class="main-content">
                <Switch<Route> render={switch} />
            </main>
        </BrowserRouter>
    }
}

pub fn main() {
    yew::Renderer::<App>::new().render();
}
