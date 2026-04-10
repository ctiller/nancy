pub mod repo;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title, MetaTags};
use leptos_router::components::{Router, Route, Routes};
use leptos_router::path;

#[component]
pub fn Shell() -> impl IntoView {
    provide_meta_context();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <MetaTags/>
                {
                    #[cfg(feature = "ssr")]
                    {
                        let options = leptos::prelude::use_context::<leptos::config::LeptosOptions>()
                            .expect("LeptosOptions missing in SSR context");
                        view! { <HydrationScripts options=options/> }.into_any()
                    }
                    #[cfg(not(feature = "ssr"))]
                    {
                        view! { "" }.into_any()
                    }
                }
                <Stylesheet id="leptos" href="/pkg/nancy.css"/>
                <Title text="Nancy Coordinator UI"/>
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <div id="root">
            <Router>
                <Navbar />
                <main class="main-content">
                    <Routes fallback=NotFound>
                        <Route path=path!("") view=CommandView/>
                        <Route path=path!("tasks") view=TasksView/>
                        <Route path=path!("agents") view=AgentsView/>
                        <Route path=path!("repo") view=RepoView/>
                        <Route path=path!("logs") view=SettingsLogsView/>
                    </Routes>
                </main>
            </Router>
        </div>
    }
}

#[component]
fn Navbar() -> impl IntoView {
    view! {
        <nav class="glass-nav">
            <div class="nav-brand">
                <img src="/nancy-avatar.png" alt="Nancy Logo" class="brand-logo" />
                <span>"NANCY"</span>
            </div>
            
            <div class="nav-links">
                <a href="/" class="nav-item">"Command"</a>
                <a href="/tasks" class="nav-item">"Tasks"</a>
                <a href="/agents" class="nav-item">"Agents"</a>
                <a href="/repo" class="nav-item">"Repo"</a>
                <a href="/logs" class="nav-item">"Settings & Logs"</a>
            </div>
            
            <div class="status-indicator">
                <div class="status-dot"></div>
                <span>"Coordinator Active"</span>
            </div>
        </nav>
    }
}

#[component]
fn CommandView() -> impl IntoView {
    view! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px;">
                <h3>"Pending Inquiries"</h3>
                <p class="text-muted">"No active agent questions at this time."</p>
                <button style="position: absolute; bottom: 24px; left: 24px;" class="glass-panel">"+ New Task"</button>
            </div>
            <div class="glass-panel" style="padding: 20px;">
                <h3>"Workspace Editor"</h3>
                <p>"Select an inquiry or start a new task."</p>
            </div>
        </div>
    }
}

#[component]
fn TasksView() -> impl IntoView {
    view! {
        <div class="glass-panel" style="height: 100%; padding: 20px;">
            <h2>"Task Topology Graph"</h2>
            <p>"DAG visualization rendering here..."</p>
        </div>
    }
}

#[component]
fn AgentsView() -> impl IntoView {
    view! {
        <div class="glass-panel" style="padding: 20px;">
            <h2>"Active Grinders"</h2>
            <div style="display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 16px;">
                <div class="glass-panel" style="padding: 16px;">"Grinder: Alpha"</div>
                <div class="glass-panel" style="padding: 16px;">"Grinder: Beta"</div>
            </div>
        </div>
    }
}

#[component]
fn RepoView() -> impl IntoView {
    let (active_file, set_active_file) = signal::<Option<String>>(None);
    let (selected_branch, set_selected_branch) = signal::<Option<String>>(None);
    
    let branches_ctx = Resource::new(
        move || (),
        move |_| async move {
            crate::repo::get_git_branches().await.unwrap_or(crate::repo::GitBranchContext {
                active_branch: "master".to_string(),
                all_branches: vec!["master".to_string()]
            })
        }
    );
    
    view! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px; overflow-y: auto;">
                <div style="display: flex; flex-direction: column; gap: 12px; margin-bottom: 16px;">
                    <h3 style="margin: 0; white-space: nowrap; flex-shrink: 0;">"Repository Explorer"</h3>
                    <Suspense fallback=move || view! { <span>"..."</span> }>
                        {move || {
                            branches_ctx.get().map(|ctx| {
                                view! {
                                    <select 
                                        class="branch-select"
                                        style="background: rgba(0,0,0,0.2); color: var(--text-main); border: 1px solid var(--panel-border); padding: 6px 8px; border-radius: 4px; outline: none; font-size: 0.85rem; width: 100%; box-sizing: border-box; text-overflow: ellipsis; overflow: hidden; white-space: nowrap;"
                                        on:change=move |ev| {
                                            let branch = event_target_value(&ev);
                                            set_selected_branch.set(Some(branch));
                                            set_active_file.set(None);
                                        }
                                    >
                                        <For
                                            each=move || ctx.all_branches.clone()
                                            key=|br| br.clone()
                                            children={let active = ctx.active_branch.clone(); move |br| {
                                                let br_clone = br.clone();
                                                let br_val = br.clone();
                                                view! {
                                                    <option value=br_val selected={br == active}>{br_clone}</option>
                                                }
                                            }}
                                        />
                                    </select>
                                }
                            })
                        }}
                    </Suspense>
                </div>
                <Suspense fallback=move || view! { <span>"Loading Tree..."</span> }>
                    {move || {
                        let curr_branch = selected_branch.get().unwrap_or_else(|| {
                            branches_ctx.get().map(|c| c.active_branch).unwrap_or_default()
                        });
                        view! {
                            <FileTree current_dir=None set_active_file=set_active_file branch=curr_branch />
                        }
                    }}
                </Suspense>
            </div>
            <div class="glass-panel code-inspector" style="padding: 0; overflow-y: auto;">
                <Suspense fallback=move || view! { <span>"..."</span> }>
                    {move || {
                        let curr_branch = selected_branch.get().unwrap_or_else(|| {
                            branches_ctx.get().map(|c| c.active_branch).unwrap_or_default()
                        });
                        view! {
                            <FileInspector active_file=active_file branch=curr_branch />
                        }
                    }}
                </Suspense>
            </div>
        </div>
    }
}

#[component]
fn FileTree(
    current_dir: Option<String>,
    set_active_file: WriteSignal<Option<String>>,
    branch: String
) -> impl IntoView {
    let dir_clone = current_dir.clone();
    let branch_clone = branch.clone();
    let files = Resource::new(
        move || branch_clone.clone(),
        move |b| crate::repo::get_repo_tree(b, dir_clone.clone())
    );

    let branch_for_match = branch.clone();
    view! {
        <Suspense fallback=move || view! { <div>"Loading..."</div> }>
            <div class="file-tree" style="margin-left: 12px; font-family: monospace; font-size: 0.9rem;">
                {move || match files.get() {
                    Some(Ok(nodes)) => {
                        let nodes_clone = nodes.clone();
                        let branch_for_for = branch_for_match.clone();
                        view! {
                            <For
                                each=move || nodes_clone.clone()
                                key=|node| node.path.clone()
                                children=move |node| {
                                    let is_dir = node.is_dir;
                                    let path = node.path.clone();
                                    let name = node.name.clone();
                                    let (expanded, set_expanded) = signal(false);
                                    let branch_c = branch_for_for.clone();
                                    
                                    view! {
                                        <div class="file-node" style="margin-top: 4px;">
                                            <div 
                                                style=move || format!("cursor: pointer; padding: 4px; border-radius: 4px; display:flex; gap: 8px; align-items:center; {}", if is_dir { "font-weight: bold; color: var(--accent-cyan);" } else { "" })
                                                on:click=move |_| {
                                                    if is_dir {
                                                        set_expanded.update(|e| *e = !*e);
                                                    } else {
                                                        set_active_file.set(Some(path.clone()));
                                                    }
                                                }
                                            >
                                                <span style="font-size: 1.1rem;">{move || if is_dir { if expanded.get() { "📂" } else { "📁" } } else { "📄" }}</span>
                                                <span>{name}</span>
                                            </div>
                                            <Show when=move || expanded.get()>
                                                {
                                                    let p = node.path.clone();
                                                    let set_act = set_active_file;
                                                    let b_pass = branch_c.clone();
                                                    move || view! { <FileTree current_dir=Some(p.clone()) set_active_file=set_act branch=b_pass.clone() /> }
                                                }
                                            </Show>
                                        </div>
                                    }
                                }
                            />
                        }.into_any()
                    },
                    Some(Err(e)) => view! { <div style="color: red;">{format!("Error: {:?}", e)}</div> }.into_any(),
                    None => view! { <div>"..."</div> }.into_any(),
                }}
            </div>
        </Suspense>
    }
}

#[component]
fn FileInspector(
    active_file: ReadSignal<Option<String>>,
    branch: String
) -> impl IntoView {
    let branch_clone = branch.clone();
    let file_content = Resource::new(
        move || (active_file.get(), branch_clone.clone()),
        move |(path, b)| async move {
            match path {
                Some(p) => crate::repo::read_file_text(b, p).await,
                None => Ok("".to_string()),
            }
        }
    );

    view! {
        <Suspense fallback=move || view! { <div style="padding: 20px;">"Loading..."</div> }>
            {move || {
                let target = active_file.get();
                if let Some(path) = target {
                    let p_lower = path.to_lowercase();
                    if p_lower.ends_with(".png") || p_lower.ends_with(".jpg") || p_lower.ends_with(".svg") {
                        return view! {
                            <div style="padding: 24px; display: flex; justify-content: center; align-items: center; min-height: 100%;">
                                <img src=format!("/api/fs/{}", path) style="max-width: 100%; border-radius: 8px; box-shadow: 0 4px 12px rgba(0,0,0,0.5);" />
                            </div>
                        }.into_any();
                    }
                    
                    match file_content.get() {
                        Some(Ok(html)) => view! {
                            <div style="padding: 16px; font-family: monospace; font-size: 0.9rem;" inner_html=html></div>
                        }.into_any(),
                        Some(Err(e)) => view! {
                            <div style="padding: 20px; color: #f43f5e;">"Fail: " {format!("{:?}", e)}</div>
                        }.into_any(),
                        None => view! { <div></div> }.into_any()
                    }
                } else {
                    view! {
                        <div style="padding: 20px; color: var(--text-muted); display:flex; align-items:center; justify-content:center; height:100%;">
                            "Select a file to inspect."
                        </div>
                    }.into_any()
                }
            }}
        </Suspense>
    }
}

#[component]
fn SettingsLogsView() -> impl IntoView {
    view! {
        <div class="glass-panel" style="padding: 20px;">
            <h2>"System Metrics & Configuration"</h2>
            <p>"Live execution logs and environment key settings."</p>
        </div>
    }
}

#[component]
fn NotFound() -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        // Set HTTP status code 404
        if let Some(resp) = use_context::<leptos_axum::ResponseOptions>() {
            resp.set_status(axum::http::StatusCode::NOT_FOUND);
        }
    }
    view! {
        <h1>"404 - Not Found"</h1>
        <p>"The page you are looking for does not exist."</p>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    // Initializes panic hook and logger.
    console_error_panic_hook::set_once();
    leptos::logging::log!("Nancy Web UI Hydrating...");
    leptos::mount::hydrate_body(App);
}
