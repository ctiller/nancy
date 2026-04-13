use serde::{Deserialize, Serialize};
use yew::prelude::*;

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = window)]
    fn mountReadOnlyMonaco(id: &str, content: &str, path: &str);

    #[wasm_bindgen::prelude::wasm_bindgen(js_namespace = window)]
    fn parseMarkdown(md: &str) -> String;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GitBranchContext {
    pub active_branch: String,
    pub all_branches: Vec<String>,
}

#[derive(Properties, PartialEq)]
pub struct FileTreeProps {
    pub branch: String,
    pub current_dir: Option<String>,
    pub on_select_file: Callback<String>,
}

#[function_component(FileTree)]
pub fn file_tree(props: &FileTreeProps) -> Html {
    let nodes = use_state(|| None::<Vec<FileNode>>);
    let error = use_state(|| None::<String>);

    {
        let branch = props.branch.clone();
        let dir = props.current_dir.clone();
        let nodes = nodes.clone();
        let error = error.clone();

        use_effect_with((branch.clone(), dir.clone()), move |(b, d)| {
            nodes.set(None);
            error.set(None);
            let b_pass = b.clone();
            let d_pass = d.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut url = format!("/api/repo/tree?branch={}", urlencoding::encode(&b_pass));
                if let Some(dir_path) = d_pass {
                    url.push_str(&format!("&dir={}", urlencoding::encode(&dir_path)));
                }

                match gloo_net::http::Request::get(&url).send().await {
                    Ok(resp) => {
                        if resp.ok() {
                            if let Ok(data) = resp.json::<Vec<FileNode>>().await {
                                nodes.set(Some(data));
                            } else {
                                error.set(Some("Parse error".to_string()));
                            }
                        } else {
                            error.set(Some(format!("Server error: {}", resp.status())));
                        }
                    }
                    Err(e) => {
                        error.set(Some(e.to_string()));
                    }
                }
            });
            || ()
        });
    }

    html! {
        <div class="file-tree" style="margin-left: 12px; font-family: monospace; font-size: 0.9rem;">
            if let Some(err) = &*error {
                <div style="color: red;">{format!("Error: {}", err)}</div>
            } else if let Some(files) = &*nodes {
                { for files.iter().map(|node| {
                    let is_dir = node.is_dir;
                    let path = node.path.clone();
                    let name = node.name.clone();

                    html! {
                        <FileNodeItem is_dir={is_dir} path={path} name={name} branch={props.branch.clone()} on_select_file={props.on_select_file.clone()} />
                    }
                })}
            } else {
                <div>{"Loading..."}</div>
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
struct FileNodeItemProps {
    is_dir: bool,
    path: String,
    name: String,
    branch: String,
    on_select_file: Callback<String>,
}

#[function_component(FileNodeItem)]
fn file_node_item(props: &FileNodeItemProps) -> Html {
    let expanded = use_state(|| false);

    let is_dir = props.is_dir;
    let path = props.path.clone();

    let onclick = {
        let expanded = expanded.clone();
        let on_select = props.on_select_file.clone();
        let path_clone = path.clone();
        Callback::from(move |_| {
            if is_dir {
                expanded.set(!*expanded);
            } else {
                on_select.emit(path_clone.clone());
            }
        })
    };

    let style = format!(
        "cursor: pointer; padding: 4px; border-radius: 4px; display:flex; gap: 8px; align-items:center; {}",
        if is_dir {
            "font-weight: bold; color: var(--accent-cyan);"
        } else {
            ""
        }
    );

    html! {
        <div class="file-node" style="margin-top: 4px;">
            <div style={style} onclick={onclick}>
                <span style="font-size: 1.1rem;">
                    {if is_dir { if *expanded { "📂" } else { "📁" } } else { "📄" }}
                </span>
                <span>{&props.name}</span>
            </div>
            if *expanded {
                <FileTree current_dir={Some(path)} branch={props.branch.clone()} on_select_file={props.on_select_file.clone()} />
            }
        </div>
    }
}

#[derive(Properties, PartialEq)]
pub struct FileInspectorProps {
    pub active_file: Option<String>,
    pub branch: String,
}

#[function_component(FileInspector)]
pub fn file_inspector(props: &FileInspectorProps) -> Html {
    let file_content = use_state(|| None::<String>);
    let error = use_state(|| None::<String>);

    {
        let path_opt = props.active_file.clone();
        let branch = props.branch.clone();
        let file_content = file_content.clone();
        let error = error.clone();

        use_effect_with((path_opt.clone(), branch.clone()), move |(path, b)| {
            if let Some(p) = path {
                file_content.set(None);
                error.set(None);
                let p_pass = p.clone();
                let b_pass = b.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let url = format!(
                        "/api/repo/file?branch={}&path={}",
                        urlencoding::encode(&b_pass),
                        urlencoding::encode(&p_pass)
                    );
                    match gloo_net::http::Request::get(&url).send().await {
                        Ok(resp) => {
                            if resp.ok() {
                                if let Ok(text) = resp.text().await {
                                    file_content.set(Some(text));
                                }
                            } else {
                                error.set(Some(format!("Server error: {}", resp.status())));
                            }
                        }
                        Err(e) => {
                            error.set(Some(e.to_string()));
                        }
                    }
                });
            }
            || ()
        });
    }

    {
        let content_opt = file_content.clone();
        let path_opt = props.active_file.clone();
        use_effect_with((content_opt.clone(), path_opt.clone()), move |(content, path)| {
            if let (Some(c), Some(p)) = (&**content, path) {
                if !p.to_lowercase().ends_with(".md") && !p.to_lowercase().ends_with(".png") && !p.to_lowercase().ends_with(".jpg") && !p.to_lowercase().ends_with(".svg") {
                    mountReadOnlyMonaco("repo-monaco", c, p);
                }
            }
            || ()
        });
    }

    html! {
        if let Some(path) = &props.active_file {
            if path.to_lowercase().ends_with(".png") || path.to_lowercase().ends_with(".jpg") || path.to_lowercase().ends_with(".svg") {
                <div style="padding: 24px; display: flex; justify-content: center; align-items: center; min-height: 100%;">
                    <img src={format!("/api/fs/{}", path)} style="max-width: 100%; border-radius: 8px; box-shadow: 0 4px 12px rgba(0,0,0,0.5);" />
                </div>
            } else if let Some(err) = &*error {
                <div style="padding: 20px; color: #f43f5e;">{"Fail: "}{err}</div>
            } else if let Some(content) = &*file_content {
                if path.to_lowercase().ends_with(".md") {
                    <div style="padding: 16px; font-family: sans-serif; font-size: 1rem; color: var(--text-main); height: 100%; overflow-y: auto;" class="markdown-body">
                        {yew::Html::from_html_unchecked(AttrValue::from(parseMarkdown(content)))}
                    </div>
                } else {
                    <div id="repo-monaco" style="height: 100%; min-height: 500px; width: 100%;"></div>
                }
            } else {
                <div style="padding: 20px;">{"Loading..."}</div>
            }
        } else {
            <div style="padding: 20px; color: var(--text-muted); display:flex; align-items:center; justify-content:center; height:100%;">
                {"Select a file to inspect."}
            </div>
        }
    }
}

#[function_component(RepoView)]
pub fn repo_view() -> Html {
    let active_file = use_state(|| None::<String>);
    let selected_branch = use_state(|| None::<String>);

    let branches_ctx = use_state(|| None::<GitBranchContext>);

    {
        let branches_ctx = branches_ctx.clone();
        use_effect_with((), move |_| {
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(resp) = gloo_net::http::Request::get("/api/repo/branches")
                    .send()
                    .await
                {
                    if let Ok(data) = resp.json::<GitBranchContext>().await {
                        branches_ctx.set(Some(data));
                    }
                }
            });
            || ()
        });
    }

    let branch_to_use = selected_branch.as_ref().cloned().unwrap_or_else(|| {
        branches_ctx
            .as_ref()
            .map(|c| c.active_branch.clone())
            .unwrap_or_else(|| "master".to_string())
    });

    let on_select_file = {
        let active_file = active_file.clone();
        Callback::from(move |path: String| {
            active_file.set(Some(path));
        })
    };

    let on_branch_change = {
        let selected_branch = selected_branch.clone();
        let active_file = active_file.clone();
        Callback::from(move |e: Event| {
            use wasm_bindgen::JsCast;
            let target = e.target().unwrap();
            let select = target.unchecked_into::<web_sys::HtmlSelectElement>();
            selected_branch.set(Some(select.value()));
            active_file.set(None);
        })
    };

    html! {
        <div class="grid-2">
            <div class="glass-panel" style="padding: 20px; overflow-y: auto;">
                <div style="display: flex; flex-direction: column; gap: 12px; margin-bottom: 16px;">
                    <h3 style="margin: 0; white-space: nowrap; flex-shrink: 0;">{"Repository Explorer"}</h3>
                    if let Some(ctx) = &*branches_ctx {
                        <select
                            class="branch-select"
                            style="background: rgba(0,0,0,0.2); color: var(--text-main); border: 1px solid var(--panel-border); padding: 6px 8px; border-radius: 4px; outline: none; font-size: 0.85rem; width: 100%; box-sizing: border-box; text-overflow: ellipsis; overflow: hidden; white-space: nowrap;"
                            onchange={on_branch_change}
                        >
                            { for ctx.all_branches.iter().map(|br| {
                                html! {
                                    <option value={br.clone()} selected={br == &branch_to_use}>{br.clone()}</option>
                                }
                            })}
                        </select>
                    } else {
                        <span>{"..."}</span>
                    }
                </div>
                <FileTree current_dir={None::<String>} branch={branch_to_use.clone()} on_select_file={on_select_file.clone()} />
            </div>
            <div class="glass-panel code-inspector" style="padding: 0; overflow-y: auto;">
                <FileInspector active_file={(*active_file).clone()} branch={branch_to_use} />
            </div>
        </div>
    }
}
