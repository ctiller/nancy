use leptos::prelude::*;
use crate::schema::{TopologyResponse, TopologyNode, TopologyEdge, NodeType};

// Frontend layout dimensions
const NODE_WIDTH: f64 = 280.0;
const NODE_HEIGHT: f64 = 80.0;

#[component]
pub fn TasksView() -> impl IntoView {
    let (rendered_nodes, set_rendered_nodes) = signal::<Vec<TopologyNode>>(Vec::new());
    let (rendered_edges, set_rendered_edges) = signal::<Vec<TopologyEdge>>(Vec::new());
    
    // SVG coordinate bounds mapped for panning/scrolling
    let (max_width, set_max_width) = signal::<f64>(800.0);
    let (max_height, set_max_height) = signal::<f64>(600.0);

    #[cfg(feature = "hydrate")]
    {
        leptos::task::spawn_local(async move {
            let mut last_version: Option<u64> = None;
            loop {
                let url = if let Some(lv) = last_version {
                    format!("/api/tasks/topology?last_version={}", lv)
                } else {
                    "/api/tasks/topology".to_string()
                };
                
                if let Ok(resp) = gloo_net::http::Request::get(&url).send().await {
                    if resp.status() == 200 {
                        if let Ok(text) = resp.text().await {
                            match serde_json::from_str::<TopologyResponse>(&text) {
                                Ok(data) => {
                                    last_version = Some(data.version);
                                    
                                    set_max_width.set(data.max_width);
                                    set_max_height.set(data.max_height);
                                    set_rendered_nodes.set(data.nodes);
                                    set_rendered_edges.set(data.edges);
                                    continue;
                                }
                                Err(e) => {
                                    leptos::logging::log!("Failed to parse topology JSON: {}", e);
                                    leptos::logging::log!("Response text: {}", text);
                                }
                            }
                        }
                    } else {
                        leptos::logging::log!("Failed topology fetch status: {}", resp.status());
                    }
                } else {
                    leptos::logging::log!("Network fetch for topology completely failed");
                }
                
                gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
            }
        });
    }

    view! {
        <div class="glass-panel" style="height: calc(100vh - 140px); min-height: 600px; width: 100%; overflow: auto; position: relative;">
            <svg 
                width=move || max_width.get()
                height=move || max_height.get()
                style="position: absolute; top: 0; left: 0; min-height: 100%; min-width: 100%;"
            >
                <g class="edges">
                    <For
                        each=move || rendered_edges.get()
                        key=|edge| format!("{}->{}", edge.source, edge.target)
                        children=move |edge| {
                            let pts = edge.points.clone();
                            let d = if pts.is_empty() {
                                "".to_string()
                            } else {
                                let mut path = format!("M {} {}", pts[0].0, pts[0].1);
                                for p in pts.iter().skip(1) {
                                    path.push_str(&format!(" L {} {}", p.0, p.1));
                                }
                                path
                            };
                            
                            view! {
                                <path 
                                    d=d
                                    stroke="rgba(255, 255, 255, 0.25)"
                                    stroke-width="2"
                                    fill="none"
                                    stroke-dasharray="4 4"
                                    style="transition: all 0.35s ease-out;"
                                />
                            }
                        }
                    />
                </g>

                <g class="nodes">
                    <For
                        each=move || rendered_nodes.get()
                        key=|node| node.id.clone()
                        children=move |n| {
                            
                            let (bg_color, border_color) = if n.is_completed {
                                ("rgba(40, 40, 40, 0.8)", "rgba(100, 100, 100, 0.4)")
                            } else {
                                match n.node_type {
                                    NodeType::Plan => ("rgba(147, 51, 234, 0.15)", "rgba(147, 51, 234, 0.5)"), // Purple
                                    NodeType::TaskRequest => ("rgba(249, 115, 22, 0.15)", "rgba(249, 115, 22, 0.6)"), // Orange
                                    NodeType::Task => ("rgba(6, 182, 212, 0.15)", "rgba(6, 182, 212, 0.5)"), // Cyan
                                }
                            };

                            let pulsate_class = if n.active_agent.is_some() && !n.is_completed {
                                match n.node_type {
                                    NodeType::Task | NodeType::Plan => "pulsate-cyan",
                                    NodeType::TaskRequest => "pulsate-orange",
                                }
                            } else {
                                ""
                            };

                            view! {
                                <g 
                                    style=move || format!(
                                        "pointer-events: auto; transform: translate({}px, {}px); transition: transform 0.35s ease-out, opacity 0.35s;", 
                                        n.x, n.y
                                    )
                                    class=format!("task-node {}", pulsate_class)
                                >
                                    <rect 
                                        x=-(NODE_WIDTH / 2.0)
                                        y=-(NODE_HEIGHT / 2.0)
                                        width=NODE_WIDTH
                                        height=NODE_HEIGHT
                                        rx="8"
                                        fill=bg_color
                                        stroke=border_color
                                        stroke-width="2"
                                        style="backdrop-filter: blur(8px);"
                                    />
                                    
                                    // Header boundary
                                    <text x=-(NODE_WIDTH / 2.0) + 12.0 y=-(NODE_HEIGHT / 2.0) + 24.0 fill="var(--text-main)" font-family="monospace" font-size="0.8rem" font-weight="bold">
                                        {format!("{:?}", n.node_type).to_uppercase()}
                                    </text>
                                    
                                    // Status or Assigned Agent
                                    <text x=(NODE_WIDTH / 2.0) - 12.0 y=-(NODE_HEIGHT / 2.0) + 24.0 fill="var(--text-muted)" font-family="monospace" font-size="0.75rem" text-anchor="end">
                                        {
                                            if n.is_completed { 
                                                "COMPLETED".to_string() 
                                            } else if let Some(agent) = n.active_agent { 
                                                format!("Assigned: {}", &agent[..8.min(agent.len())]) 
                                            } else { 
                                                "PENDING".to_string() 
                                            }
                                        }
                                    </text>

                                    // Content Text
                                    <foreignObject x=-(NODE_WIDTH / 2.0) + 12.0 y=-(NODE_HEIGHT / 2.0) + 32.0 width=(NODE_WIDTH - 24.0) height=(NODE_HEIGHT - 40.0)>
                                        <div style="color: var(--text-muted); font-size: 0.85rem; font-family: monospace; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; text-overflow: ellipsis; padding-top: 4px;">
                                            {n.name}
                                        </div>
                                    </foreignObject>
                                </g>
                            }
                        }
                    />
                </g>
            </svg>
        </div>
    }
}
