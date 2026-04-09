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
    view! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px;">
                <h3>"Repository Explorer"</h3>
                <p>"/master"</p>
            </div>
            <div class="glass-panel" style="padding: 20px;">
                <h3>"Code Inspector"</h3>
            </div>
        </div>
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
