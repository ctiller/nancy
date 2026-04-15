// Copyright 2026 Craig Tiller
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use schema::MarketStateResponse;
use yew::prelude::*;

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
                    if cancel_clone.get() {
                        break;
                    }

                    let mut req = gloo_net::http::Request::get("/api/market/state");
                    if let Some(sig) = &signal {
                        req = req.abort_signal(Some(sig));
                    }
                    if let Ok(resp) = req.send().await {
                        if resp.ok() {
                            if let Ok(text) = resp.text().await {
                                match serde_json::from_str::<MarketStateResponse>(&text) {
                                    Ok(data) => {
                                        market_state.set(Some(data));
                                    }
                                    Err(e) => {
                                        web_sys::console::error_1(&format!("Logs parse error: {}", e).into());
                                    }
                                }
                            }
                        } else {
                            web_sys::console::error_1(&format!("Logs status error: {}", resp.status()).into());
                        }
                    }

                    if cancel_clone.get() {
                        break;
                    }
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
                                <div style="display: flex; justify-content: space-between; align-items: center;">
                                    <h3>{"Model Consumption Metrics"}</h3>
                                    <div style="text-align: right;">
                                        <div style="font-size: 1.1em; color: var(--accent-light); font-weight: bold;">
                                            { format!("Current Budget: ${:.2}", state.budget_pool_nanocents.0 as f64 / 100_000_000_000.0) }
                                        </div>
                                        <div style="font-size: 0.9em; color: var(--text-muted);">
                                            { format!("Available (less expected costs): ${:.2}", (state.budget_pool_nanocents.0 as f64 - state.inflight_costs_nanocents.0 as f64) / 100_000_000_000.0) }
                                        </div>
                                    </div>
                                </div>
                                <table style="width: 100%; border-collapse: collapse; text-align: left;">
                                    <thead>
                                        <tr style="border-bottom: 1px solid rgba(255,255,255,0.2);">
                                            <th style="padding: 10px;">{"Model"}</th>
                                            <th style="padding: 10px;">{"Status"}</th>
                                            <th style="padding: 10px;">{"Quotas (RPM/TPM/RPD)"}</th>
                                            <th style="padding: 10px;">{"Expected Grant (Req/Tok/Cost)"}</th>
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
                                                <td style="padding: 10px;">
                                                    <span style={match stats.status.as_str() {
                                                        "Healthy" => "color: #4caf50; font-weight: bold;",
                                                        "Unhealthy" => "color: #f44336; font-weight: bold;",
                                                        "Recovering" => "color: #ff9800; font-weight: bold;",
                                                        _ => "color: var(--text-muted);"
                                                    }}>
                                                        {stats.status.clone()}
                                                    </span>
                                                </td>
                                                <td style="padding: 10px;" >
                                                    { format!("{} / {} / {}",
                                                        stats.active_quotas.rpm.map(|v| format!("{:.0}", v)).unwrap_or_else(|| "Inf".to_string()),
                                                        stats.active_quotas.tpm.map(|v| if v >= 1_000_000.0 { format!("{:.1}M", v/1_000_000.0) } else if v >= 1000.0 { format!("{:.0}k", v/1000.0) } else { format!("{:.0}", v) }).unwrap_or_else(|| "Inf".to_string()),
                                                        stats.active_quotas.rpd.map(|v| if v >= 1_000_000.0 { format!("{:.1}M", v/1_000_000.0) } else if v >= 1000.0 { format!("{:.0}k", v/1000.0) } else { format!("{:.0}", v) }).unwrap_or_else(|| "Inf".to_string())
                                                    ) }
                                                </td>
                                                <td style="padding: 10px;">
                                                    { format!("{:.1} / {} / ${:.4}",
                                                        stats.expected_grant_requests,
                                                        fmt_tok(stats.expected_grant_tokens as u64),
                                                        stats.expected_grant_cost)
                                                    }
                                                </td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_1m.input_tokens + stats.trailing_1m.output_tokens), stats.trailing_1m.requests, stats.trailing_1m.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_3m.input_tokens + stats.trailing_3m.output_tokens), stats.trailing_3m.requests, stats.trailing_3m.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_10m.input_tokens + stats.trailing_10m.output_tokens), stats.trailing_10m.requests, stats.trailing_10m.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_30m.input_tokens + stats.trailing_30m.output_tokens), stats.trailing_30m.requests, stats.trailing_30m.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
                                                <td style="padding: 10px;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.trailing_100m.input_tokens + stats.trailing_100m.output_tokens), stats.trailing_100m.requests, stats.trailing_100m.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
                                                <td style="padding: 10px; font-weight: bold;">{ format!("{} / {} / ${:.4}", fmt_tok(stats.total.input_tokens + stats.total.output_tokens), stats.total.requests, stats.total.cost_nanocents.0 as f64 / 100_000_000_000.0) }</td>
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
                                            let current_now = (js_sys::Date::now() / 1000.0) as u64;
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
                                    <h3>{ "Subagent Costs" }</h3>
                                    <div style="display: flex; flex-direction: column; gap: 10px; max-height: 400px; overflow-y: auto;">
                                        <table style="width: 100%; border-collapse: collapse; text-align: left;">
                                            <thead>
                                                <tr style="border-bottom: 1px solid rgba(255,255,255,0.2);">
                                                    <th style="padding: 10px;">{"Subagent Path"}</th>
                                                    <th style="padding: 10px;">{"Total Cost USD"}</th>
                                                </tr>
                                            </thead>
                                            <tbody>
                                                { for state.subagent_costs.iter().map(|(agent, cost)| {
                                                    html! {
                                                        <tr style="border-bottom: 1px solid rgba(255,255,255,0.05);">
                                                            <td style="padding: 10px; font-family: monospace; word-break: break-all;">{ agent.clone() }</td>
                                                            <td style="padding: 10px; font-weight: bold; color: var(--accent-light);">{ format!("${:.5}", cost) }</td>
                                                        </tr>
                                                    }
                                                })}
                                            </tbody>
                                        </table>
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

// DOCUMENTED_BY: [docs/adr/0060-llm-streaming-introspection-and-ledger-rollup.md]
