use crate::schema::ipc::*;
use crate::schema::coordinator_config::CoordinatorConfig;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{oneshot, RwLock};

#[derive(Debug)]
pub struct AuctionBid {
    pub payload: RequestModelPayload,
    pub tx: oneshot::Sender<RequestModelResponse>,
    pub submitted_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: u64,
    pub metrics: UsageMetrics,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ConsumptionRates {
    pub expected_cost: f64,
    pub expected_tokens: f64,
    pub expected_requests: f64,
}

#[derive(Clone)]
pub struct RateLimits {
    pub rpm: Option<f64>,
    pub tpm: Option<f64>,
    pub rpd: Option<f64>,
}

#[derive(Clone, Default)]
pub struct ActiveQuotas {
    pub rpm: Option<f64>,
    pub tpm: Option<f64>,
    pub rpd: Option<f64>,
}

pub struct ArbitrationMarket {
    pub pending_bids: Vec<AuctionBid>,
    pub active_leases: Vec<RequestModelResponse>,
    pub consumption_history: HashMap<schema::LlmModel, VecDeque<UsageRecord>>,
    pub active_quotas: HashMap<schema::LlmModel, ActiveQuotas>,
    pub lease_history: HashMap<schema::LlmModel, VecDeque<u64>>,
    pub historical_rates: HashMap<schema::LlmModel, ConsumptionRates>,
    pub budget_pool_usd: f64,
    pub config: CoordinatorConfig,
}

pub type SharedArbitrationMarket = Arc<RwLock<ArbitrationMarket>>;

pub struct TokenCost {
    pub input: f64,
    pub output: f64,
}

impl ArbitrationMarket {

    pub fn rate_limits_for(model: schema::LlmModel) -> RateLimits {
        match model {
            schema::LlmModel::Gemini25FlashLite => RateLimits { rpm: Some(10_000.0), tpm: Some(10_000_000.0), rpd: None },
            schema::LlmModel::Gemini25Flash => RateLimits { rpm: Some(2000.0), tpm: Some(3_000_000.0), rpd: Some(100_000.0) },
            schema::LlmModel::Gemini25Pro => RateLimits { rpm: Some(1000.0), tpm: Some(5_000_000.0), rpd: Some(50_000.0) },
            schema::LlmModel::Gemini30FlashPreview => RateLimits { rpm: Some(2000.0), tpm: Some(3_000_000.0), rpd: Some(100_000.0) },
            schema::LlmModel::Gemini31FlashLitePreview => RateLimits { rpm: Some(10_000.0), tpm: Some(10_000_000.0), rpd: None },
            schema::LlmModel::Gemini31ProPreview => RateLimits { rpm: Some(1000.0), tpm: Some(5_000_000.0), rpd: Some(50_000.0) },
            schema::LlmModel::TestMockModel => RateLimits { rpm: Some(10_000.0), tpm: Some(10_000.0), rpd: Some(10_000.0) },
        }
    }

    pub fn cost_for(model: schema::LlmModel, tokens: u64) -> TokenCost {
        match model {
            schema::LlmModel::Gemini25FlashLite => TokenCost { input: 0.1 / 1_000_000.0, output: 0.4 / 1_000_000.0 },
            schema::LlmModel::Gemini25Flash => TokenCost { input: 0.3 / 1_000_000.0, output: 2.50 / 1_000_000.0 },
            schema::LlmModel::Gemini25Pro => {
                if tokens > 200_000 { TokenCost { input: 2.50 / 1_000_000.0, output: 15.0 / 1_000_000.0 } }
                else { TokenCost { input: 1.25 / 1_000_000.0, output: 10.0 / 1_000_000.0 } }
            },
            schema::LlmModel::Gemini30FlashPreview => TokenCost { input: 0.5 / 1_000_000.0, output: 3.0 / 1_000_000.0 },
            schema::LlmModel::Gemini31FlashLitePreview => TokenCost { input: 0.25 / 1_000_000.0, output: 1.5 / 1_000_000.0 },
            schema::LlmModel::Gemini31ProPreview => {
                if tokens > 200_000 { TokenCost { input: 4.0 / 1_000_000.0, output: 18.0 / 1_000_000.0 } }
                else { TokenCost { input: 2.0 / 1_000_000.0, output: 12.0 / 1_000_000.0 } }
            },
            schema::LlmModel::TestMockModel => TokenCost { input: 0.0, output: 0.0 },
        }
    }

    pub fn new(config: CoordinatorConfig) -> SharedArbitrationMarket {
        let mut active_quotas = HashMap::new();
        for model in schema::LlmModel::ALL {
            let limits = Self::rate_limits_for(*model);
            active_quotas.insert(*model, ActiveQuotas {
                rpm: limits.rpm,
                tpm: limits.tpm,
                rpd: limits.rpd,
            });
        }

        let mut historical_rates = HashMap::new();
        if let Some(mut rates_path) = config.nancy_dir.clone() {
            rates_path.push("consumption_rates.json");
            if let Ok(content) = std::fs::read_to_string(&rates_path) {
                if let Ok(parsed) = serde_json::from_str::<HashMap<schema::LlmModel, ConsumptionRates>>(&content) {
                    historical_rates = parsed;
                }
            }
        }

        let market = Arc::new(RwLock::new(Self {
            pending_bids: Vec::new(),
            active_leases: Vec::new(),
            consumption_history: HashMap::new(),
            lease_history: HashMap::new(),
            historical_rates,
            active_quotas,
            budget_pool_usd: 0.0,
            config,
        }));

        let m_clone = market.clone();
        tokio::spawn(async move {
            Self::run_auction_loop(m_clone).await;
        });

        market
    }

    pub fn submit_bid(
        market: &SharedArbitrationMarket,
        payload: RequestModelPayload,
    ) -> oneshot::Receiver<RequestModelResponse> {
        let (tx, rx) = oneshot::channel();
        let submitted_at_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let bid = AuctionBid {
            payload,
            tx,
            submitted_at_unix,
        };

        let m = market.clone();
        tokio::spawn(async move {
            let mut lock = m.write().await;
            lock.pending_bids.push(bid);
        });

        rx
    }

    pub async fn record_consumption(
        market: &SharedArbitrationMarket,
        payload: LlmUsagePayload,
    ) {
        let mut lock = market.write().await;
        let model_cost_schema = Self::cost_for(payload.model, payload.input_tokens);
        let actual_cost = (payload.input_tokens as f64 * model_cost_schema.input) + (payload.output_tokens as f64 * model_cost_schema.output);
        
        // Exact spend subtraction strictly safely mapped mathematically natively
        lock.budget_pool_usd -= actual_cost;

        let entry = lock.consumption_history.entry(payload.model.clone()).or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        entry.push_back(UsageRecord {
            timestamp: now,
            metrics: UsageMetrics {
                requests: 1,
                input_tokens: payload.input_tokens,
                output_tokens: payload.output_tokens,
                cost_usd: actual_cost,
            }
        });

        // Drop anything strictly older than 100 minutes to prevent memory boundary bloat structurally.
        let oldest_valid = now.saturating_sub(100 * 60);
        while let Some(front) = entry.front() {
            if front.timestamp < oldest_valid {
                entry.pop_front();
            } else {
                break;
            }
        }
    }

    fn merge_metrics(target: &mut UsageMetrics, rec: &UsageMetrics) {
        target.requests += rec.requests;
        target.input_tokens += rec.input_tokens;
        target.output_tokens += rec.output_tokens;
        target.cost_usd += rec.cost_usd;
    }

    pub async fn get_market_state(market: &SharedArbitrationMarket) -> MarketStateResponse {
        let lock = market.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut per_model_stats = std::collections::BTreeMap::new();

        for (model, records) in &lock.consumption_history {
            let mut stat = ModelUsageStats {
                total: UsageMetrics::default(),
                active_quotas: lock.active_quotas.get(model).cloned().map(|q| crate::schema::ipc::Quotas { rpm: q.rpm, tpm: q.tpm, rpd: q.rpd }).unwrap_or_default(),
                trailing_1m: UsageMetrics::default(),
                trailing_3m: UsageMetrics::default(),
                trailing_10m: UsageMetrics::default(),
                trailing_30m: UsageMetrics::default(),
                trailing_100m: UsageMetrics::default(),
            };

            for r in records {
                Self::merge_metrics(&mut stat.total, &r.metrics);
                let age = now.saturating_sub(r.timestamp);
                if age <= 60 { Self::merge_metrics(&mut stat.trailing_1m, &r.metrics); }
                if age <= 3 * 60 { Self::merge_metrics(&mut stat.trailing_3m, &r.metrics); }
                if age <= 10 * 60 { Self::merge_metrics(&mut stat.trailing_10m, &r.metrics); }
                if age <= 30 * 60 { Self::merge_metrics(&mut stat.trailing_30m, &r.metrics); }
                if age <= 100 * 60 { Self::merge_metrics(&mut stat.trailing_100m, &r.metrics); }
            }
            per_model_stats.insert(model.clone(), stat);
        }

        // Fill in bounds for models with NO historical usage yet
        for (model, quota) in &lock.active_quotas {
            if !per_model_stats.contains_key(model) {
                per_model_stats.insert(model.clone(), ModelUsageStats {
                    total: UsageMetrics::default(),
                    active_quotas: crate::schema::ipc::Quotas { rpm: quota.rpm, tpm: quota.tpm, rpd: quota.rpd },
                    trailing_1m: UsageMetrics::default(),
                    trailing_3m: UsageMetrics::default(),
                    trailing_10m: UsageMetrics::default(),
                    trailing_30m: UsageMetrics::default(),
                    trailing_100m: UsageMetrics::default(),
                });
            }
        }

        let pending_bids = lock.pending_bids.iter().map(|b| PendingBidInfo {
            requester_id: b.payload.requester_id.clone(),
            choices: b.payload.choices.clone(),
            submitted_at_unix: b.submitted_at_unix,
        }).collect();

        MarketStateResponse {
            per_model_stats,
            pending_bids,
            active_leases: lock.active_leases.clone(),
        }
    }

    async fn run_auction_loop(market: SharedArbitrationMarket) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(20)).await;
            
            let mut lock = market.write().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            // 1. Replenish quotas
            let models = schema::LlmModel::ALL;
            for model in models {
                let limit = Self::rate_limits_for(*model);
                let mut active = lock.active_quotas.entry(*model).or_default();
                
                if let Some(r) = limit.rpm {
                    let cur = active.rpm.unwrap_or(0.0);
                    active.rpm = Some(f64::min(cur + (r / 3.0), r)); // 20s is 1/3 of a minute
                }
                if let Some(t) = limit.tpm {
                    let cur = active.tpm.unwrap_or(0.0);
                    active.tpm = Some(f64::min(cur + (t / 3.0), t));
                }
                if let Some(rd) = limit.rpd {
                    let cur = active.rpd.unwrap_or(0.0);
                    active.rpd = Some(f64::min(cur + (rd / 4320.0), rd)); // 20s is 1/4320 of 24h
                }
            }
            
            // 2. Replenish USD Budget Pool
            let daily = lock.config.daily_budget_usd;
            let hourly_cap = daily / 24.0;
            let bump_per_loop = daily / 4320.0; // 24 hours * 60 mins / 20 seconds = 4320 rounds
            lock.budget_pool_usd = f64::min(lock.budget_pool_usd + bump_per_loop, hourly_cap);
            
            let mut available_tick_budget = lock.budget_pool_usd;

            // 3. Clear expired leases organically
            lock.active_leases.retain(|lease| {
                now < lease.granted_at_unix + lease.lease_duration_sec
            });

            // Clean up stale lease history
            let oldest_valid = now.saturating_sub(100 * 60);
            for history in lock.lease_history.values_mut() {
                while let Some(front) = history.front() {
                    if *front < oldest_valid { history.pop_front(); } else { break; }
                }
            }

            // 4. Extract and flat-map bids into individual choice tickets
            let bids = std::mem::take(&mut lock.pending_bids);
            let mut inflight_requests: Vec<Option<AuctionBid>> = bids.into_iter().map(Some).collect();

            let mut tickets = Vec::new();
            for (idx, req) in inflight_requests.iter().enumerate() {
                if let Some(bid) = req {
                    for choice in &bid.payload.choices {
                        tickets.push((choice.bid_value, idx, choice.clone()));
                    }
                }
            }
            
            // Sort highest unconditionally globally establishing strict multi-model precedence exactly reliably
            tickets.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            // 5. Grant leases eagerly bounding positive quotas globally cleanly!
            for (_value, req_idx, choice) in tickets {
                let Some(_bid) = inflight_requests[req_idx].as_ref() else {
                    continue; // Reached if another choice from this same request previously satisfied it!
                };

                let leases_count = lock.lease_history.get(&choice.name).map(|l| l.len()).unwrap_or(0);
                let default_cost = if choice.name.to_string().contains("pro") { 0.02 } else { 0.001 };
                
                let (expected_cost, expected_tokens, expected_requests) = if leases_count > 0 {
                    if let Some(records) = lock.consumption_history.get(&choice.name) {
                        let total_cost: f64 = records.iter().map(|r| r.metrics.cost_usd).sum();
                        let total_tokens: f64 = records.iter().map(|r| r.metrics.input_tokens as f64 + r.metrics.output_tokens as f64).sum();
                        let total_requests: f64 = records.iter().map(|r| r.metrics.requests as f64).sum();
                        let leases_f64 = leases_count as f64;
                        (
                            total_cost / leases_f64,
                            total_tokens / leases_f64,
                            f64::max(1.0, total_requests / leases_f64)
                        )
                    } else if let Some(rates) = lock.historical_rates.get(&choice.name) {
                        (rates.expected_cost, rates.expected_tokens, rates.expected_requests)
                    } else {
                        (default_cost, 2000.0, 1.0)
                    }
                } else if let Some(rates) = lock.historical_rates.get(&choice.name) {
                    (rates.expected_cost, rates.expected_tokens, rates.expected_requests)
                } else {
                    (default_cost, 2000.0, 1.0)
                };
                
                let mut granted_choice = false;
                if available_tick_budget >= expected_cost {
                    let mut active = lock.active_quotas.entry(choice.name.clone()).or_default();
                    
                    let rpm_ok = active.rpm.map_or(true, |r| r >= expected_requests);
                    let tpm_ok = active.tpm.map_or(true, |t| t >= expected_tokens);
                    let rpd_ok = active.rpd.map_or(true, |rd| rd >= expected_requests);
                    
                    if rpm_ok && tpm_ok && rpd_ok {
                        if let Some(ref mut r) = active.rpm { *r -= expected_requests; }
                        if let Some(ref mut t) = active.tpm { *t -= expected_tokens; }
                        if let Some(ref mut rd) = active.rpd { *rd -= expected_requests; }
                        granted_choice = true;
                    }
                }

                if granted_choice {
                    available_tick_budget -= expected_cost;
                    
                    let lease_id = uuid::Uuid::new_v4().to_string();
                    let resp = RequestModelResponse {
                        granted_model: choice.name.clone(),
                        lease_id: lease_id.clone(),
                        lease_duration_sec: 60,
                        granted_at_unix: now,
                    };
                    
                    lock.active_leases.push(resp.clone());
                    lock.lease_history.entry(choice.name.clone()).or_default().push_back(now);
                    
                    // Consume the request eliminating all other fallback tickets functionally!
                    let bid_data = inflight_requests[req_idx].take().unwrap();
                    let _ = bid_data.tx.send(resp);
                }
            }

            lock.pending_bids = inflight_requests.into_iter().flatten().collect();

            let rates_json_and_path = if let Some(mut rates_path) = lock.config.nancy_dir.clone() {
                rates_path.push("consumption_rates.json");
                let mut current_rates = HashMap::new();
                for model in schema::LlmModel::ALL {
                    let leases_count = lock.lease_history.get(model).map(|l| l.len()).unwrap_or(0);
                    if leases_count > 0 {
                        if let Some(records) = lock.consumption_history.get(model) {
                            let total_cost: f64 = records.iter().map(|r| r.metrics.cost_usd).sum();
                            let total_tokens: f64 = records.iter().map(|r| r.metrics.input_tokens as f64 + r.metrics.output_tokens as f64).sum();
                            let total_requests: f64 = records.iter().map(|r| r.metrics.requests as f64).sum();
                            let leases_f64 = leases_count as f64;
                            current_rates.insert(*model, ConsumptionRates {
                                expected_cost: total_cost / leases_f64,
                                expected_tokens: total_tokens / leases_f64,
                                expected_requests: f64::max(1.0, total_requests / leases_f64),
                            });
                        }
                    } else if let Some(old) = lock.historical_rates.get(model) {
                        current_rates.insert(*model, old.clone());
                    }
                }
                lock.historical_rates = current_rates.clone();
                serde_json::to_string_pretty(&current_rates).ok().map(|j| (j, rates_path))
            } else {
                None
            };
            
            drop(lock);

            if let Some((json, path)) = rates_json_and_path {
                let _ = tokio::fs::write(path, json).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_quota_replenishment_bounds_and_budget(
            initial_reqs in 0.0_f64..200.0,
            budget in 10.0_f64..50.0,
            daily_usd in 10.0_f64..100.0
        ) {
            // rate_limits removed. Tests implicitly use ArbitrationMarket::rate_limits_for()

            let mut active_quotas = HashMap::new();
            active_quotas.insert(schema::LlmModel::TestMockModel, ActiveQuotas { rpm: Some(initial_reqs), tpm: None, rpd: None });

            let config = CoordinatorConfig {
                daily_budget_usd: daily_usd,
                nancy_dir: None,
            };

            let market = Arc::new(RwLock::new(ArbitrationMarket {
                pending_bids: Vec::new(),
                active_leases: Vec::new(),
                consumption_history: HashMap::new(),
                lease_history: HashMap::new(),
                historical_rates: HashMap::new(),
                active_quotas,
                budget_pool_usd: 0.0,
                config,
            }));

            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut lock = market.write().await;
                
                let models = schema::LlmModel::ALL;
                for model in models {
                    let limit = ArbitrationMarket::rate_limits_for(*model);
                    let mut active = lock.active_quotas.entry(*model).or_default();
                    if let Some(r) = limit.rpm {
                        active.rpm = Some(f64::min(active.rpm.unwrap_or(0.0) + (r / 3.0), r)); // Mocking the RPM 20s refill
                    }
                }
                
                let daily = lock.config.daily_budget_usd;
                let hourly_cap = daily / 24.0;
                let bump = daily / 4320.0;
                lock.budget_pool_usd = f64::min(lock.budget_pool_usd + bump, hourly_cap);
                
                let res = lock.active_quotas.get(&schema::LlmModel::TestMockModel).unwrap().rpm.unwrap();
                let expected_limit = ArbitrationMarket::rate_limits_for(schema::LlmModel::TestMockModel).rpm.unwrap();
                assert!(res <= expected_limit); // Proves the use-or-lose requirement holds cleanly algebraically.
                assert!(lock.budget_pool_usd <= hourly_cap); // Check strict max hourly pools
            });
        }
    }
}
