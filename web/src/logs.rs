use yew::prelude::*;
use schema::MarketStateResponse;

#[function_component(LogsView)]
pub fn logs_view() -> Html {
    let market_state = use_state(|| None::<MarketStateResponse>);

    {
        let market_state = market_state.clone();
        
        use_effect_with((), move |_| {
            let cancelled = std::rc::Rc::new(std::cell::Cell::new(false));
            let cancel_clone = cancelled.clone();
            let abort_controller = web_sys::AbortController::new().ok();
            let signal = abort_controller.as_ref().map(|ac| ac.signal());

            wasm_bindgen_futures::spawn_local(async move {
                loop {
                    if cancel_clone.get() { break; }
                    
                    let mut req = gloo_net::http::Request::get("/api/market/state");
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }
                    if let Ok(resp) = req.send().await {
                        if resp.ok() {
                            if let Ok(data) = resp.json::<MarketStateResponse>().await {
                                market_state.set(Some(data));
                            }
                        }
                    }

                    if cancel_clone.get() { break; }
                    gloo_timers::future::sleep(std::time::Duration::from_millis(1000)).await;
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

    html! {
        <div style="padding: 20px; overflow-y: auto; height: 100%;">
            <h2>{"Token Market Dashboard"}</h2>
            {
                if let Some(state) = &*market_state {
                    html! {
                        <div style="display: flex; flex-direction: column; gap: 24px;">
                            <div class="glass-panel" style="padding: 20px;">
                                <h3>{"Model Consumption Metrics"}</h3>
                                <table style="width: 100%; border-collapse: collapse; text-align: left;">
                                    <thead>
                                        <tr style="border-bottom: 1px solid rgba(255,255,255,0.2);">
                                            <th style="padding: 10px;">{"Model"}</th>
                                            <th style="padding: 10px;">{"Quotas (RPM/TPM/RPD)"}</th>
                                            <th style="padding: 10px;">{"1m (Tok/Req/$)"}</th>
                                            <th style="padding: 10px;">{"3m (Tok/Req/$)"}</th>
                                            <th style="padding: 10px;">{"10m (Tok/Req/$)"}</th>
                                            <th style="padding: 10px;">{"30m (Tok/Req/$)"}</th>
                                            <th style="padding: 10px;">{"100m (Tok/Req/$)"}</th>
                                            <th style="padding: 10px;">{"Total (Tok/Req/$)"}</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        { for state.per_model_stats.iter().map(|(model, stats)| {
                                            let fmt_tok = |t: u64| {
                                                if t >= 1_000_000 { format!("{:.1}M", t as f64 / 1_000_000.0) }
                                                else if t >= 1000 { format!("{:.0}k", t as f64 / 1000.0) }
                                                else { t.to_string() }
                                            };
                                            html! {
                                            <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                                                <td style="padding: 10px; font-family: monospace;">{ model.to_string() }</td>
                                                <td style="padding: 10px;" >
                                                    { format!("{} / {} / {}", 
                                                        stats.active_quotas.rpm.map(|v| format!("{:.0}", v)).unwrap_or_else(|| "Inf".to_string()),
                                                        stats.active_quotas.tpm.map(|v| if v >= 1_000_000.0 { format!("{:.1}M", v/1_000_000.0) } else if v >= 1000.0 { format!("{:.0}k", v/1000.0) } else { format!("{:.0}", v) }).unwrap_or_else(|| "Inf".to_string()),
                                                        stats.active_quotas.rpd.map(|v| if v >= 1_000_000.0 { format!("{:.1}M", v/1_000_000.0) } else if v >= 1000.0 { format!("{:.0}k", v/1000.0) } else { format!("{:.0}", v) }).unwrap_or_else(|| "Inf".to_string())
                                                    ) }
                                                </td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_1m.input_tokens + stats.trailing_1m.output_tokens), stats.trailing_1m.requests, stats.trailing_1m.cost_usd) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_3m.input_tokens + stats.trailing_3m.output_tokens), stats.trailing_3m.requests, stats.trailing_3m.cost_usd) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_10m.input_tokens + stats.trailing_10m.output_tokens), stats.trailing_10m.requests, stats.trailing_10m.cost_usd) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_30m.input_tokens + stats.trailing_30m.output_tokens), stats.trailing_30m.requests, stats.trailing_30m.cost_usd) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_100m.input_tokens + stats.trailing_100m.output_tokens), stats.trailing_100m.requests, stats.trailing_100m.cost_usd) }</td>
                                                <td style="padding: 10px; font-weight: bold;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.total.input_tokens + stats.total.output_tokens), stats.total.requests, stats.total.cost_usd) }</td>
                                            </tr>
                                        }})}
                                    </tbody>
                                </table>
                            </div>
                            
                            <div style="display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 24px;">
                                <div class="glass-panel" style="padding: 20px;">
                                    <h3>{ format!("Pending Bids ({})", state.pending_bids.len()) }</h3>
                                    <div style="display: flex; flex-direction: column; gap: 10px; max-height: 400px; overflow-y: auto;">
                                        { for state.pending_bids.iter().map(|bid| {
                                            let current_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                            html! {
                                            <div style="background: rgba(0,0,0,0.3); padding: 12px; border-radius: 6px; border-left: 4px solid var(--accent-orange, #ff9800);">
                                                <div style="font-weight: bold; margin-bottom: 4px;">{ format!("Subagent: {}", bid.requester_id) }</div>
                                                <div style="font-size: 0.85em; color: var(--text-muted);">
                                                    { "Age: " }
                                                    { format!("{}s", current_now.saturating_sub(bid.submitted_at_unix)) }
                                                </div>
                                                <div style="margin-top: 8px; font-size: 0.9em; font-family: monospace;">
                                                    { for bid.choices.iter().map(|choice| html! {
                                                        <div>{ format!("{} @ {:.2}", choice.name, choice.bid_value) }</div>
                                                    })}
                                                </div>
                                            </div>
                                        }})}
                                    </div>
                                </div>
                                
                                <div class="glass-panel" style="padding: 20px;">
                                    <h3>{ format!("Active Leases ({})", state.active_leases.len()) }</h3>
                                    <div style="display: flex; flex-direction: column; gap: 10px; max-height: 400px; overflow-y: auto;">
                                        { for state.active_leases.iter().map(|lease| {
                                            let current_now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                                            let expires_at = lease.granted_at_unix + lease.lease_duration_sec;
                                            html! {
                                            <div style="background: rgba(0,0,0,0.3); padding: 12px; border-radius: 6px; border-left: 4px solid var(--accent-light);">
                                                <div style="font-weight: bold; margin-bottom: 4px;">{ format!("Model: {}", lease.granted_model) }</div>
                                                <div style="font-size: 0.85em; color: var(--text-muted);">
                                                    { "Lease: " } { lease.lease_id.clone() }
                                                </div>
                                                <div style="font-size: 0.85em; color: var(--text-muted);">
                                                    { "Expires in: " }
                                                    {
                                                        if expires_at > current_now {
                                                            format!("{}s", expires_at - current_now)
                                                        } else {
                                                            "Expired (Awaiting cleanup)".to_string()
                                                        }
                                                    }
                                                </div>
                                            </div>
                                        }})}
                                    </div>
                                </div>
                            </div>
                        </div>
                    }
                } else {
                    html! {
                        <div style="display: flex; align-items: center; justify-content: center; height: 200px;">
                            <div class="status-dot" style="margin-right: 12px;"></div>
                            <span style="color: var(--text-muted);">{"Connecting to Token Market..."}</span>
                        </div>
                    }
                }
            }
        </div>
    }
}
