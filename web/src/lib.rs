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
                    <main>
                        <Routes fallback=NotFound>
                            <Route path=path!("") view=HomePage/>
                        </Routes>
                    </main>
                </Router>
            </body>
        </html>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <h1>"Welcome to Nancy Web UI (SSR)"</h1>
        <p>"Hello World from the Coordinator!"</p>
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
