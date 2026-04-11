use yew::prelude::*;
use schema::{TopologyResponse, TopologyNode, TopologyEdge, NodeType};

const NODE_WIDTH: f64 = 280.0;
const NODE_HEIGHT: f64 = 80.0;

#[function_component(TasksView)]
pub fn tasks_view() -> Html {
    let rendered_nodes = use_state(|| Vec::<TopologyNode>::new());
    let rendered_edges = use_state(|| Vec::<TopologyEdge>::new());
    let max_width = use_state(|| 800.0);
    let max_height = use_state(|| 600.0);
    let active_agent_statuses = use_state(|| std::collections::HashMap::<String, String>::new());

    {
        let rendered_nodes = rendered_nodes.clone();
        let rendered_edges = rendered_edges.clone();
        let max_width = max_width.clone();
        let max_height = max_height.clone();

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
                        format!("/api/tasks/topology?last_version={}", lv)
                    } else {
                        "/api/tasks/topology".to_string()
                    };
                    
                    let mut req = gloo_net::http::Request::get(&url);
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }
                    
                    if let Ok(resp) = req.send().await {
                        if cancel_clone.get() { break; }
                        if resp.ok() {
                            if let Ok(data) = resp.json::<TopologyResponse>().await {
                                last_version = Some(data.version);
                                max_width.set(data.max_width);
                                max_height.set(data.max_height);
                                rendered_nodes.set(data.nodes);
                                rendered_edges.set(data.edges);
                            }
                        }
                    }
                    if cancel_clone.get() { break; }
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

    {
        let nodes_state = rendered_nodes.clone();
        let statuses = active_agent_statuses.clone();
        let active_dids: Vec<String> = rendered_nodes.iter()
            .filter_map(|n| if !n.is_completed { n.active_agent.clone() } else { None })
            .collect();
        let active_dids_key = active_dids.join(",");
        use_effect_with(active_dids_key, move |_| {
            let active_dids: Vec<String> = nodes_state.iter()
                .filter_map(|n| if !n.is_completed { n.active_agent.clone() } else { None })
                .collect();

                
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            
            if !active_dids.is_empty() {
                 wasm_bindgen_futures::spawn_local(async move {
                     loop {
                         if cancel_clone.get() { break; }
                         let mut new_statuses = std::collections::HashMap::new();
                         for did in &active_dids {
                             let url = format!("/api/grinders/{}/state", did);
                             if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
                                 if let Ok(json) = resp.json::<serde_json::Value>().await {
                                     if let Some(frame_val) = json.get("tree") {
                                         if let Ok(frame) = serde_json::from_value::<schema::SerializedFrame>(frame_val.clone()) {
                                             if let Some(st) = frame.status {
                                                 new_statuses.insert(did.clone(), st);
                                             }
                                         }
                                     }
                                 }
                             }
                         }
                         if cancel_clone.get() { break; }
                         statuses.set(new_statuses);
                         gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                     }
                 });
            }
            
            move || {
                cancelled.set(true);
            }
        });
    }

    let nodes = rendered_nodes.iter().map(|n| {
        let (bg_color, border_color) = if n.is_completed {
            ("rgba(40, 40, 40, 0.8)", "rgba(100, 100, 100, 0.4)")
        } else {
            match n.node_type {
                NodeType::Plan => ("rgba(147, 51, 234, 0.15)", "rgba(147, 51, 234, 0.5)"), // Purple
                NodeType::TaskRequest => ("rgba(249, 115, 22, 0.15)", "rgba(249, 115, 22, 0.6)"), // Orange
                NodeType::Task => ("rgba(6, 182, 212, 0.15)", "rgba(6, 182, 212, 0.5)"), // Cyan
                NodeType::Ask => ("rgba(220, 38, 38, 0.15)", "rgba(220, 38, 38, 0.8)"), // Red
            }
        };

        let pulsate_class = if n.active_agent.is_some() && !n.is_completed {
            match n.node_type {
                NodeType::Task | NodeType::Plan => "pulsate-cyan",
                NodeType::TaskRequest => "pulsate-orange",
                NodeType::Ask => "pulsate-red",
            }
        } else {
            ""
        };

        let node_type_upper = match n.node_type {
            NodeType::Task => "TASK",
            NodeType::TaskRequest => "TASKREQUEST",
            NodeType::Plan => "PLAN",
            NodeType::Ask => "HUMAN ASK",
        };

        let status_text = if n.is_completed {
            "COMPLETED".to_string()
        } else if let Some(agent) = &n.active_agent {
            if let Some(summary) = active_agent_statuses.get(agent) {
                summary.clone()
            } else {
                format!("Assigned: {}", &agent[..8.min(agent.len())])
            }
        } else {
            "PENDING".to_string()
        };

        let transform_style = format!(
            "pointer-events: auto; transform: translate({}px, {}px); transition: transform 0.35s ease-out, opacity 0.35s;", 
            n.x, n.y
        );
        let class_str = format!("task-node {}", pulsate_class);

        html! {
            <g key={n.id.clone()} style={transform_style} class={class_str}>
                <rect 
                    x={(-(NODE_WIDTH / 2.0)).to_string()}
                    y={(-(NODE_HEIGHT / 2.0)).to_string()}
                    width={NODE_WIDTH.to_string()}
                    height={NODE_HEIGHT.to_string()}
                    rx="8"
                    fill={bg_color}
                    stroke={border_color}
                    stroke-width="2"
                    style="backdrop-filter: blur(8px);"
                />
                
                <text 
                    x={(-(NODE_WIDTH / 2.0) + 12.0).to_string()} 
                    y={(-(NODE_HEIGHT / 2.0) + 24.0).to_string()} 
                    fill="var(--text-main)" 
                    font-family="monospace" 
                    font-size="0.8rem" 
                    font-weight="bold"
                >
                    {node_type_upper}
                </text>
                
                <text 
                    x={((NODE_WIDTH / 2.0) - 12.0).to_string()} 
                    y={(-(NODE_HEIGHT / 2.0) + 24.0).to_string()} 
                    fill="var(--text-muted)" 
                    font-family="monospace" 
                    font-size="0.75rem" 
                    text-anchor="end"
                >
                    {status_text}
                </text>

                <foreignObject 
                    x={(-(NODE_WIDTH / 2.0) + 12.0).to_string()} 
                    y={(-(NODE_HEIGHT / 2.0) + 32.0).to_string()} 
                    width={(NODE_WIDTH - 24.0).to_string()} 
                    height={(NODE_HEIGHT - 40.0).to_string()}
                >
                    <div style="color: var(--text-muted); font-size: 0.85rem; font-family: monospace; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; text-overflow: ellipsis; padding-top: 4px;">
                        {n.name.clone()}
                    </div>
                </foreignObject>
            </g>
        }
    });

    let edges = rendered_edges.iter().map(|edge| {
        let pts = edge.points.clone();
        let key = format!("{}->{}", edge.source, edge.target);
        let d = if pts.is_empty() {
            "".to_string()
        } else {
            let mut path = format!("M {} {}", pts[0].0, pts[0].1);
            for p in pts.iter().skip(1) {
                path.push_str(&format!(" L {} {}", p.0, p.1));
            }
            path
        };
        
        html! {
            <path 
                key={key}
                d={d}
                stroke="rgba(255, 255, 255, 0.25)"
                stroke-width="2"
                fill="none"
                stroke-dasharray="4 4"
                style="transition: all 0.35s ease-out;"
            />
        }
    });

    html! {
        <div class="glass-panel" style="height: calc(100vh - 140px); min-height: 600px; width: 100%; overflow: auto; position: relative;">
            <svg 
                width={(*max_width).to_string()}
                height={(*max_height).to_string()}
                style="position: absolute; top: 0; left: 0; min-height: 100%; min-width: 100%;"
            >
                <g class="edges">
                    { for edges }
                </g>

                <g class="nodes">
                    { for nodes }
                </g>
            </svg>
        </div>
    }
}
