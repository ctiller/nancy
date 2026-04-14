use schema::{NodeType, TopologyEdge, TopologyNode, TopologyResponse};
use yew::prelude::*;

const NODE_WIDTH: f64 = 280.0;
const NODE_HEIGHT: f64 = 120.0;

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
                    if cancel_clone.get() {
                        break;
                    }
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
                        if cancel_clone.get() {
                            break;
                        }
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

    {
        let nodes_state = rendered_nodes.clone();
        let statuses = active_agent_statuses.clone();
        let active_dids: Vec<String> = rendered_nodes
            .iter()
            .filter_map(|n| {
                if !n.is_completed {
                    n.active_agent.clone()
                } else {
                    None
                }
            })
            .collect();
        let active_dids_key = active_dids.join(",");
        use_effect_with(active_dids_key, move |_| {
            let active_dids: Vec<String> = nodes_state
                .iter()
                .filter_map(|n| {
                    if !n.is_completed {
                        n.active_agent.clone()
                    } else {
                        None
                    }
                })
                .collect();

            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();

            let (tx, mut rx) = futures::channel::mpsc::unbounded::<(String, String)>();

            let statuses_clone = statuses.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut current_map = std::collections::HashMap::new();
                use futures::StreamExt;
                while let Some((did, status)) = rx.next().await {
                    if cancel_clone.get() {
                        break;
                    }
                    current_map.insert(did, status);
                    statuses_clone.set(current_map.clone());
                }
            });

            let mut controllers = Vec::new();
            if !active_dids.is_empty() {
                for did in active_dids {
                    let tx = tx.clone();
                    let cancel = cancelled.clone();
                    let abort_controller = web_sys::AbortController::new().ok();
                    let signal = abort_controller.as_ref().map(|ac| ac.signal());

                    if let Some(ref ac) = abort_controller {
                        controllers.push(ac.clone());
                    }

                    wasm_bindgen_futures::spawn_local(async move {
                        let mut last_update: Option<u64> = None;
                        loop {
                            if cancel.get() {
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
                                if cancel.get() {
                                    break;
                                }
                                if resp.ok() {
                                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                                        if let (Some(new_update), Some(frame_val)) = (
                                            json.get("update_number").and_then(|v| v.as_u64()),
                                            json.get("tree"),
                                        ) {
                                            if Some(new_update) != last_update {
                                                if let Ok(frame) = serde_json::from_value::<
                                                    schema::SerializedFrame,
                                                >(
                                                    frame_val.clone()
                                                ) {
                                                    last_update = Some(new_update);
                                                    if let Some(st) = frame.rollup.or(frame.status)
                                                    {
                                                        let _ =
                                                            tx.unbounded_send((did.clone(), st));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            gloo_timers::future::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    });
                }
            }

            move || {
                cancelled.set(true);
                for ac in controllers {
                    ac.abort();
                }
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

        let wrap_text = |text: &str, max_len: usize, max_lines: usize| -> Vec<String> {
            let mut lines = Vec::new();
            let mut current_line = String::new();
            for word in text.split_whitespace() {
                if current_line.len() + word.len() + 1 > max_len {
                    if !current_line.is_empty() {
                        if lines.len() == max_lines - 1 {
                            current_line.push_str("...");
                            lines.push(current_line);
                            return lines;
                        }
                        lines.push(current_line);
                        current_line = String::new();
                    }
                }
                if !current_line.is_empty() {
                    current_line.push(' ');
                }
                
                // If single word is longer than max_len
                if current_line.is_empty() && word.len() > max_len {
                    let mut truncated = word[..max_len.min(word.len()).saturating_sub(3)].to_string();
                    truncated.push_str("...");
                    current_line.push_str(&truncated);
                } else {
                    current_line.push_str(word);
                }
            }
            if !current_line.is_empty() && lines.len() < max_lines {
                lines.push(current_line);
            }
            lines
        };

        let name_lines = wrap_text(&n.name, 35, 2);
        let status_lines = wrap_text(&status_text, 36, 3);

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

                { if n.cost_nanocents.0 > 0 {
                    let cost_str = format!("${:.4}", n.cost_nanocents.0 as f64 / 100_000_000_000.0);
                    html! {
                        <text 
                            x={((NODE_WIDTH / 2.0) - 12.0).to_string()} 
                            y={(-(NODE_HEIGHT / 2.0) + 24.0).to_string()} 
                            fill="red" 
                            font-family="monospace" 
                            font-size="0.8rem" 
                            font-weight="bold"
                            text-anchor="end"
                        >
                            {cost_str}
                        </text>
                    }
                } else {
                    html! {}
                } }

                <text 
                    fill="var(--text-muted)" 
                    font-family="monospace" 
                    font-size="0.85rem"
                >
                    { for name_lines.into_iter().enumerate().map(|(i, line)| {
                        html! {
                            <tspan 
                                x={(-(NODE_WIDTH / 2.0) + 12.0).to_string()} 
                                y={(-(NODE_HEIGHT / 2.0) + 48.0 + (i as f64 * 16.0)).to_string()}
                            >
                                {line}
                            </tspan>
                        }
                    }) }
                </text>

                <text 
                    fill="var(--text-muted)" 
                    font-family="monospace" 
                    font-size="0.75rem"
                >
                    { for status_lines.into_iter().enumerate().map(|(i, line)| {
                        html! {
                            <tspan 
                                x={(-(NODE_WIDTH / 2.0) + 12.0).to_string()} 
                                y={(-(NODE_HEIGHT / 2.0) + 82.0 + (i as f64 * 14.0)).to_string()}
                            >
                                {line}
                            </tspan>
                        }
                    }) }
                </text>
            </g>
        }
    });

    fn format_spline(pts: &[(f64, f64)]) -> String {
        if pts.len() < 2 {
            return "".to_string();
        }

        let mut ortho_pts: Vec<(f64, f64)> = vec![];
        ortho_pts.push(pts[0]);
        for i in 0..pts.len() - 1 {
            let p0 = pts[i];
            let p1 = pts[i + 1];
            if (p0.0 - p1.0).abs() > 1.0 && (p0.1 - p1.1).abs() > 1.0 {
                // Safely transit across the empty Y mid-point corridor between ranks to guarantee no node collisions
                let mid_y = (p0.1 + p1.1) / 2.0;
                ortho_pts.push((p0.0, mid_y));
                ortho_pts.push((p1.0, mid_y));
            }
            ortho_pts.push(p1);
        }

        // Strip duplicate vertices
        let mut clean_pts: Vec<(f64, f64)> = vec![];
        for pt in ortho_pts {
            if let Some(last) = clean_pts.last() {
                if (pt.0 - last.0).abs() < 1.0 && (pt.1 - last.1).abs() < 1.0 {
                    continue;
                }
            }
            clean_pts.push(pt);
        }

        let mut path = format!("M {:.2},{:.2}", clean_pts[0].0, clean_pts[0].1);
        let radius = 16.0_f64;

        // Paint lines and interpolate 90-degree bends with beautifully smooth rounded arcs
        for i in 1..clean_pts.len() - 1 {
            let p_prev = clean_pts[i - 1];
            let p_curr = clean_pts[i];
            let p_next = clean_pts[i + 1];

            let d1x = p_prev.0 - p_curr.0;
            let d1y = p_prev.1 - p_curr.1;
            let len1 = (d1x * d1x + d1y * d1y).sqrt();

            let d2x = p_next.0 - p_curr.0;
            let d2y = p_next.1 - p_curr.1;
            let len2 = (d2x * d2x + d2y * d2y).sqrt();

            if len1 < 0.1 || len2 < 0.1 {
                continue;
            }

            let r = radius.min(len1 / 2.0).min(len2 / 2.0);

            let p_start = (p_curr.0 + (d1x / len1) * r, p_curr.1 + (d1y / len1) * r);
            let p_end = (p_curr.0 + (d2x / len2) * r, p_curr.1 + (d2y / len2) * r);

            path.push_str(&format!(" L {:.2},{:.2}", p_start.0, p_start.1));
            // Intercept vertex with a buttery soft Quadratic bezier corner
            path.push_str(&format!(
                " Q {:.2},{:.2} {:.2},{:.2}",
                p_curr.0, p_curr.1, p_end.0, p_end.1
            ));
        }

        let last = clean_pts.last().unwrap();
        path.push_str(&format!(" L {:.2},{:.2}", last.0, last.1));

        path
    }

    let edges = rendered_edges.iter().map(|edge| {
        let pts = edge.points.clone();
        let key = format!("{}->{}", edge.source, edge.target);
        let d = format_spline(&pts);

        html! {
            <path
                key={key}
                d={d}
                stroke="rgba(255, 255, 255, 0.4)"
                stroke-width="2.5"
                fill="none"
                stroke-dasharray="0"
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
