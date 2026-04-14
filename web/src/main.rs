use wasm_bindgen::prelude::*;
use yew::prelude::*;
use yew_router::prelude::*;

pub mod agents;
pub mod logs;
pub mod repo;
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

#[derive(Debug, serde::Deserialize)]
struct PendingActions {
    asks: Vec<schema::AskPayload>,
    plan_reviews: Vec<schema::ReviewPlanPayload>,
}

#[derive(Clone, PartialEq)]
enum ActionSelection {
    None,
    NewTask,
    Ask(schema::AskPayload),
    PlanReview(schema::ReviewPlanPayload),
}

#[function_component(CommandView)]
fn command_view() -> Html {
    let pending_actions = use_state(|| PendingActions {
        asks: vec![],
        plan_reviews: vec![],
    });
    let selected_action = use_state(|| ActionSelection::None);

    // Main polling loop for Pending Actions
    {
        let pending_actions = pending_actions.clone();

        use_effect_with((), move |_| {
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            let abort_controller = web_sys::AbortController::new().ok();
            let signal = abort_controller.as_ref().map(|ac| ac.signal());

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    if cancel_clone.get() {
                        break;
                    }

                    let mut req_pending = gloo_net::http::Request::get("/api/human/pending");
                    if let Some(sig) = &signal {
                        req_pending = req_pending.abort_signal(Some(sig));
                    }
                    if let Ok(resp) = req_pending.send().await {
                        if resp.ok() {
                            if let Ok(data) = resp.json::<PendingActions>().await {
                                pending_actions.set(data);
                            }
                        }
                    }

                    if cancel_clone.get() {
                        break;
                    }
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
        let selected = (*selected_action).clone();
        use_effect_with(selected, move |sel| {
            if !matches!(sel, ActionSelection::None) {
                mountMonaco("monaco-container");
            }
            || ()
        });
    }

    let on_new_task = {
        let selected_action = selected_action.clone();
        Callback::from(move |_| {
            selected_action.set(ActionSelection::NewTask);
        })
    };

    let on_submit_task = {
        let selected_action = selected_action.clone();
        Callback::from(move |_| {
            let task_desc = getMonacoValue();
            let selected_action = selected_action.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let payload = schema::TaskRequestPayload {
                    requestor: "Admin Web UI".to_string(),
                    description: task_desc,
postconditions: vec![],
            };
                if let Ok(resp) = gloo_net::http::Request::post("/api/tasks")
                    .json(&payload)
                    .unwrap()
                    .send()
                    .await
                {
                    if resp.ok() {
                        selected_action.set(ActionSelection::None);
                    }
                }
            });
        })
    };

    let on_submit_response = {
        let selected = selected_action.clone();
        Callback::from(move |_| {
            let text = getMonacoValue();
            let item_ref = match &*selected {
                ActionSelection::Ask(a) => a.item_ref.clone(),
                ActionSelection::PlanReview(p) => p.plan_ref.clone(),
                _ => return,
            };
            let sel_clone = selected.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let req_body = serde_json::json!({
                    "item_ref": item_ref,
                    "text_response": Some(text)
                });
                if let Ok(resp) = gloo_net::http::Request::post("/api/human/action")
                    .json(&req_body)
                    .unwrap()
                    .send()
                    .await
                {
                    if resp.ok() {
                        sel_clone.set(ActionSelection::None);
                    }
                }
            });
        })
    };

    html! {
        <div style="display: grid; grid-template-columns: 350px 1fr; gap: 20px; height: 100%;">
            // Left Pane: Needs Action Menu
            <div class="glass-panel" style="padding: 20px; position: relative; overflow-y: auto; display: flex; flex-direction: column;">
                <h3>{"Needs Action"}</h3>

                <button class="btn-glow" style="margin-bottom: 20px; width: 100%;" onclick={on_new_task}>
                    {"+ Start a New Task"}
                </button>

                <div style="flex: 1; overflow-y: auto;">
                    if pending_actions.asks.is_empty() && pending_actions.plan_reviews.is_empty() {
                        <p class="text-muted" style="text-align: center; margin-top: 40px;">{"No pending actions at this time."}</p>
                    } else {
                        <div style="display: flex; flex-direction: column; gap: 12px;">
                            if !pending_actions.plan_reviews.is_empty() {
                                <h4 style="margin: 0; color: var(--accent-light);">{"Plan Reviews"}</h4>
                                { for pending_actions.plan_reviews.iter().map(|plan| {
                                    let p = plan.clone();
                                    let sel = selected_action.clone();
                                    let on_click = Callback::from(move |_| {
                                        sel.set(ActionSelection::PlanReview(p.clone()));
                                        let pc = p.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let _ = gloo_net::http::Request::post("/api/human/action")
                                                .json(&serde_json::json!({ "item_ref": pc.plan_ref, "text_response": None::<String> }))
                                                .unwrap().send().await;
                                        });
                                    });
                                    html! {
                                        <div class="action-card action-card-plan" onclick={on_click}>
                                            <div class="action-card-title">
                                                <span style="color: var(--accent-cyan); margin-right: 6px;">{"Plan:"}</span>
                                                { plan.document.title.clone() }
                                            </div>
                                            <div class="action-card-subtitle">
                                                <span>{"Click to review design"}</span>
                                                <span style="margin-left: auto;">{"➔"}</span>
                                            </div>
                                        </div>
                                    }
                                })}
                            }

                            if !pending_actions.asks.is_empty() {
                                <h4 style="margin: 10px 0 0 0; color: var(--accent-orange, #ff9800);">{"Agent Asks"}</h4>
                                { for pending_actions.asks.iter().map(|ask| {
                                    let a = ask.clone();
                                    let sel = selected_action.clone();
                                    let on_click = Callback::from(move |_| {
                                        sel.set(ActionSelection::Ask(a.clone()));
                                        let ac = a.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let _ = gloo_net::http::Request::post("/api/human/action")
                                                .json(&serde_json::json!({ "item_ref": ac.item_ref, "text_response": None::<String> }))
                                                .unwrap().send().await;
                                        });
                                    });
                                    html! {
                                        <div class="action-card action-card-ask" onclick={on_click}>
                                            <div class="action-card-title">
                                                <span style="color: #ff9800; margin-right: 6px;">{"Task:"}</span>
                                                { ask.task_name.clone() }
                                            </div>
                                            <div class="action-card-subtitle">
                                                <span>{"Click to answer"}</span>
                                                <span style="margin-left: auto;">{"➔"}</span>
                                            </div>
                                        </div>
                                    }
                                })}
                            }
                        </div>
                    }
                </div>
            </div>

            // Right Pane: Interaction Content
            <div class="glass-panel" style="padding: 30px; display: flex; flex-direction: column; overflow-y: auto;">
                { match &*selected_action {
                    ActionSelection::None => html! {
                        <div style="display: flex; flex: 1; align-items: center; justify-content: center;">
                            <p style="color: var(--text-muted); font-size: 1.1rem;">{"Select an item from the left to interact."}</p>
                        </div>
                    },
                    ActionSelection::NewTask => html! {
                        <div style="display: flex; flex-direction: column; height: 100%;">
                            <h3 style="margin-top: 0;">{"Workspace Editor"}</h3>
                            <div id="monaco-container" style="flex: 1; width: 100%; border-radius: 8px; overflow: hidden; border: 1px solid var(--panel-border); box-shadow: inset 0 0 10px rgba(0,0,0,0.5);"></div>
                            <div style="display: flex; justify-content: flex-end; gap: 12px; margin-top: 20px;">
                                <button class="btn-secondary" onclick={
                                    let sel = selected_action.clone();
                                    Callback::from(move |_| sel.set(ActionSelection::None))
                                }>{"Cancel"}</button>
                                <button class="btn-primary" onclick={on_submit_task}>{"Submit Task"}</button>
                            </div>
                        </div>
                    },
                    ActionSelection::Ask(ask) => html! {
                        <div style="display: flex; flex-direction: column; height: 100%;">
                            <h3 style="margin-top: 0; color: var(--accent-orange, #ff9800);">{"Respond to Agent"}</h3>
                            <div style="background: rgba(0,0,0,0.2); padding: 16px; border-radius: 6px; margin-bottom: 20px;">
                                <div style="font-weight: bold; color: var(--text-muted); margin-bottom: 8px;">{"Question context:"}</div>
                                <div style="font-size: 1.1rem;">{ask.question.clone()}</div>
                            </div>

                            <div id="monaco-container" style="flex: 1; width: 100%; border-radius: 8px; overflow: hidden; border: 1px solid var(--panel-border); box-shadow: inset 0 0 10px rgba(0,0,0,0.5);"></div>

                            <div style="display: flex; justify-content: flex-end; align-items: center; gap: 12px; margin-top: 20px;">
                                <button class="btn-secondary" onclick={
                                    let sel = selected_action.clone();
                                    Callback::from(move |_| sel.set(ActionSelection::None))
                                }>{"Close"}</button>
                                <button class="btn-primary" onclick={on_submit_response.clone()}>
                                    {"Send Response"}
                                </button>
                            </div>
                        </div>
                    },
                    ActionSelection::PlanReview(plan) => html! {
                        <div style="display: flex; flex-direction: column; height: 100%;">
                            <h3 style="margin-top: 0; color: var(--accent-light);">{"Plan Review"}</h3>
                            <div style="flex: 1; overflow-y: auto; background: rgba(0,0,0,0.2); padding: 20px; border-radius: 6px; margin-bottom: 20px;">
                                <h2 style="margin-top: 0;">{plan.document.title.clone()}</h2>
                                <p style="font-size: 1.1rem;">{plan.document.summary.clone()}</p>

                                <h4 style="margin-top: 24px; color: var(--accent);">{"Goals"}</h4>
                                <ul>
                                    { for plan.document.goals.iter().map(|g| html!{ <li>{g}</li> }) }
                                </ul>

                                <h4 style="margin-top: 24px; color: var(--accent);">{"Proposed Design"}</h4>
                                <ul>
                                    { for plan.document.proposed_design.iter().map(|d| html!{ <li>{d}</li> }) }
                                </ul>
                            </div>

                            <div style="flex: 0 0 120px; overflow: hidden; margin-bottom: 16px; border-radius: 6px; border: 1px solid var(--panel-border); box-shadow: inset 0 0 10px rgba(0,0,0,0.5);">
                                <div id="monaco-container" style="width: 100%; height: 100%;"></div>
                            </div>

                            <div style="display: flex; justify-content: flex-end; gap: 12px;">
                                <button class="btn-secondary" onclick={
                                    let sel = selected_action.clone();
                                    Callback::from(move |_| sel.set(ActionSelection::None))
                                }>{"Close"}</button>

                                <button style="background: rgba(200, 50, 50, 0.4); border: 1px solid rgba(255,100,100,0.5); border-radius: 4px; padding: 10px 20px; color: white; cursor: pointer;"
                                    onclick={
                                        let sel = selected_action.clone();
                                        Callback::from(move |_| {
                                            let mut text = getMonacoValue();
                                            if text.trim().is_empty() {
                                                text = "Reject: Changes required on design.".to_string();
                                            }
                                            let item_ref = match &*sel {
                                                ActionSelection::PlanReview(p) => p.plan_ref.clone(),
                                                _ => return,
                                            };
                                            let sel_clone = sel.clone();
                                            wasm_bindgen_futures::spawn_local(async move {
                                                let req_body = serde_json::json!({
                                                    "item_ref": item_ref,
                                                    "text_response": Some(text)
                                                });
                                                if let Ok(resp) = gloo_net::http::Request::post("/api/human/action")
                                                    .json(&req_body)
                                                    .unwrap()
                                                    .send()
                                                    .await {
                                                    if resp.ok() {
                                                        sel_clone.set(ActionSelection::None);
                                                    }
                                                }
                                            });
                                        })
                                    }>
                                    {"Request Changes"}
                                </button>

                                <button class="btn-primary" onclick={
                                    let sel = selected_action.clone();
                                    Callback::from(move |_| {
                                        let text = "Approve".to_string();
                                        let item_ref = match &*sel {
                                            ActionSelection::PlanReview(p) => p.plan_ref.clone(),
                                            _ => return,
                                        };
                                        let sel_clone = sel.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            let req_body = serde_json::json!({
                                                "item_ref": item_ref,
                                                "text_response": Some(text)
                                            });
                                            if let Ok(resp) = gloo_net::http::Request::post("/api/human/action")
                                                .json(&req_body)
                                                .unwrap()
                                                .send()
                                                .await {
                                                if resp.ok() {
                                                    sel_clone.set(ActionSelection::None);
                                                }
                                            }
                                        });
                                    })
                                }>
                                    {"Approve Plan"}
                                </button>
                            </div>
                        </div>
                    }
                }}
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
        Route::Logs => html! { <logs::LogsView /> },
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
