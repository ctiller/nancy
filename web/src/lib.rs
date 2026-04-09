pub mod repo;

use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title, MetaTags};
use leptos_router::components::{Router, Route, Routes};
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <MetaTags/>
                <Stylesheet id="leptos" href="/pkg/nancy.css"/>
                <Title text="Nancy Coordinator UI"/>
            </head>
            <body>
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
            </body>
        </html>
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
    
    view! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px; overflow-y: auto;">
                <h3>"Repository Explorer"</h3>
                <FileTree current_dir=None set_active_file=set_active_file />
            </div>
            <div class="glass-panel code-inspector" style="padding: 0; overflow-y: auto;">
                <FileInspector active_file=active_file />
            </div>
        </div>
    }
}

#[component]
fn FileTree(
    current_dir: Option<String>,
    #[prop(into)] set_active_file: WriteSignal<Option<String>>,
) -> impl IntoView {
    let dir_clone = current_dir.clone();
    let files = Resource::new(
        move || (),
        move |_| crate::repo::get_repo_tree(dir_clone.clone())
    );

    view! {
        <Suspense fallback=move || view! { <div>"Loading..."</div> }>
            <div class="file-tree" style="margin-left: 12px; font-family: monospace; font-size: 0.9rem;">
                {move || match files.get() {
                    Some(Ok(nodes)) => {
                        nodes.into_iter().map(|node| {
                            let is_dir = node.is_dir;
                            let path = node.path.clone();
                            let name = node.name.clone();
                            let (expanded, set_expanded) = signal(false);
                            
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
                                        <FileTree current_dir=Some(node.path.clone()) set_active_file=set_active_file />
                                    </Show>
                                </div>
                            }
                        }).collect_view().into_any()
                    },
                    Some(Err(e)) => view! { <div style="color: red;">{format!("Error: {:?}", e)}</div> }.into_any(),
                    None => view! { <div>"..."</div> }.into_any(),
                }}
            </div>
        </Suspense>
    }
}

#[component]
fn FileInspector(active_file: ReadSignal<Option<String>>) -> impl IntoView {
    let file_content = Resource::new(
        move || active_file.get(),
        move |path| async move {
            match path {
                Some(p) => crate::repo::read_file_text(p).await,
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
    leptos::mount_to_body(App);
}
