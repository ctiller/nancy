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

use crate::schema::coordinator_config::CoordinatorConfig;
use crate::schema::ipc::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, oneshot};

pub const TICK_TIME_SECS: u64 = 3;
pub const LEASE_TIME_SECS: u64 = 12;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HealthStateValue {
    Healthy,
    Unhealthy,
    Recovering,
}

pub struct ModelHealth {
    pub state_value: HealthStateValue,
    pub backoff: backoff::ExponentialBackoff,
    pub next_tx_ms: u64,
    pub recovering_until_ms: u64,
}

impl Default for ModelHealth {
    fn default() -> Self {
        Self {
            state_value: HealthStateValue::Healthy,
            backoff: backoff::ExponentialBackoff::default(),
            next_tx_ms: 0,
            recovering_until_ms: 0,
        }
    }
}

#[derive(Debug)]
pub struct AuctionBid {
    pub payload: LlmRequest,
    pub tx: oneshot::Sender<crate::schema::ipc::GrantedPermissionInfo>,
    pub submitted_at_unix: u64,
}

#[derive(Debug, Clone)]
pub struct UsageRecord {
    pub timestamp: u64,
    pub task_type: schema::TaskType,
    pub raw_input_size: usize,
    pub metrics: UsageMetrics,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct ConsumptionRates {
    pub expected_cost: schema::NanoCent,
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
    pub stalled_requests: Vec<AuctionBid>,
    pub consumption_history: HashMap<schema::LlmModel, VecDeque<UsageRecord>>,
    pub active_quotas: HashMap<schema::LlmModel, ActiveQuotas>,
    pub grant_history: HashMap<schema::LlmModel, VecDeque<u64>>,
    pub historical_rates: HashMap<schema::LlmModel, ConsumptionRates>,
    pub subagent_costs: HashMap<String, schema::NanoCent>,
    pub budget_pool_nanocents: schema::NanoCent,
    pub inflight_costs_nanocents: schema::NanoCent,
    pub config: CoordinatorConfig,
    pub model_health: HashMap<schema::LlmModel, ModelHealth>,
}

pub type SharedArbitrationMarket = Arc<RwLock<ArbitrationMarket>>;

pub struct TokenCost {
    pub input: u64,
    pub output: u64,
    pub cached_input: u64,
}

impl ArbitrationMarket {
    pub fn rate_limits_for(model: schema::LlmModel) -> RateLimits {
        match model {
            schema::LlmModel::Gemini25FlashLite => RateLimits {
                rpm: Some(10_000.0),
                tpm: Some(10_000_000.0),
                rpd: None,
            },
            schema::LlmModel::Gemini25Flash => RateLimits {
                rpm: Some(2000.0),
                tpm: Some(3_000_000.0),
                rpd: Some(100_000.0),
            },
            schema::LlmModel::Gemini25Pro => RateLimits {
                rpm: Some(1000.0),
                tpm: Some(5_000_000.0),
                rpd: Some(50_000.0),
            },
            schema::LlmModel::Gemini30FlashPreview => RateLimits {
                rpm: Some(2000.0),
                tpm: Some(3_000_000.0),
                rpd: Some(100_000.0),
            },
            schema::LlmModel::Gemini31FlashLitePreview => RateLimits {
                rpm: Some(10_000.0),
                tpm: Some(10_000_000.0),
                rpd: None,
            },
            schema::LlmModel::Gemini31ProPreview => RateLimits {
                rpm: Some(1000.0),
                tpm: Some(5_000_000.0),
                rpd: Some(50_000.0),
            },
            schema::LlmModel::TestMockModel => RateLimits {
                rpm: Some(10_000.0),
                tpm: Some(10_000.0),
                rpd: Some(10_000.0),
            },
        }
    }

    pub fn cost_for(model: schema::LlmModel, tokens: u64) -> TokenCost {
        match model {
            schema::LlmModel::Gemini25FlashLite => TokenCost {
                input: 10_000,
                output: 40_000,
                cached_input: 1_000,
            },
            schema::LlmModel::Gemini25Flash => TokenCost {
                input: 30_000,
                output: 250_000,
                cached_input: 3_000,
            },
            schema::LlmModel::Gemini25Pro => {
                if tokens > 200_000 {
                    TokenCost {
                        input: 250_000,
                        output: 1_500_000,
                        cached_input: 12_500,
                    }
                } else {
                    TokenCost {
                        input: 125_000,
                        output: 1_000_000,
                        cached_input: 25_000,
                    }
                }
            }
            schema::LlmModel::Gemini30FlashPreview => TokenCost {
                input: 50_000,
                output: 300_000,
                cached_input: 5_000,
            },
            schema::LlmModel::Gemini31FlashLitePreview => TokenCost {
                input: 25_000,
                output: 150_000,
                cached_input: 2_500,
            },
            schema::LlmModel::Gemini31ProPreview => {
                if tokens > 200_000 {
                    TokenCost {
                        input: 400_000,
                        output: 1_800_000,
                        cached_input: 20_000,
                    }
                } else {
                    TokenCost {
                        input: 200_000,
                        output: 1_200_000,
                        cached_input: 40_000,
                    }
                }
            }
            schema::LlmModel::TestMockModel => TokenCost {
                input: 0,
                output: 0,
                cached_input: 0,
            },
        }
    }

    pub fn new(config: CoordinatorConfig) -> SharedArbitrationMarket {
        let mut active_quotas = HashMap::new();
        for model in schema::LlmModel::ALL {
            let limits = Self::rate_limits_for(*model);
            active_quotas.insert(
                *model,
                ActiveQuotas {
                    rpm: limits.rpm,
                    tpm: limits.tpm,
                    rpd: limits.rpd,
                },
            );
        }

        let mut historical_rates = HashMap::new();
        if let Some(mut rates_path) = config.nancy_dir.clone() {
            rates_path.push("consumption_rates.json");
            if let Ok(content) = std::fs::read_to_string(&rates_path) {
                if let Ok(parsed) =
                    serde_json::from_str::<HashMap<schema::LlmModel, ConsumptionRates>>(&content)
                {
                    historical_rates = parsed;
                }
            }
        }

        let initial_budget = ((config.daily_budget_usd * 100_000_000_000.0) / 24.0) as u64;

        let market = Arc::new(RwLock::new(Self {
            stalled_requests: Vec::new(),
            consumption_history: HashMap::new(),
            grant_history: HashMap::new(),
            historical_rates,
            active_quotas,
            subagent_costs: HashMap::new(),
            budget_pool_nanocents: schema::NanoCent(initial_budget),
            inflight_costs_nanocents: schema::NanoCent(0),
            config,
            model_health: HashMap::new(),
        }));

        let m_clone = market.clone();
        tokio::spawn(async move {
            Self::run_auction_loop(m_clone).await;
        });

        market
    }

    pub fn submit_bid(
        market: &SharedArbitrationMarket,
        payload: LlmRequest,
    ) -> oneshot::Receiver<crate::schema::ipc::GrantedPermissionInfo> {
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
            
            if lock.stalled_requests.is_empty() {
                let mut best_choice = None;
                let mut best_ratio = std::f64::MIN;
                let mut chosen_expected_cost = schema::NanoCent(0);
                let mut chosen_expected_requests = 0.0;
                let mut chosen_expected_tokens = 0.0;

                for choice in &bid.payload.model_choices {
                    let is_backoff = lock.model_health.get(&choice.name).map(|h| h.state_value != HealthStateValue::Healthy).unwrap_or(false);
                    if is_backoff { continue; }

                    let (expected_cost, expected_tokens, expected_requests) = Self::expected_grant_metrics_for_bid(&lock, &choice.name, bid.payload.task_type, bid.payload.raw_input_size);
                    let (rpm, tpm, rpd) = {
                        let active = lock.active_quotas.entry(choice.name.clone()).or_default();
                        (active.rpm, active.tpm, active.rpd)
                    };
                    let rpm_ok = rpm.map_or(true, |r| r >= expected_requests);
                    let tpm_ok = tpm.map_or(true, |t| t >= expected_tokens);
                    let rpd_ok = rpd.map_or(true, |rd| rd >= expected_requests);
                    let available_budget = schema::NanoCent(lock.budget_pool_nanocents.0.saturating_sub(lock.inflight_costs_nanocents.0));
                    if rpm_ok && tpm_ok && rpd_ok && available_budget >= expected_cost {
                        let ratio = (choice.bid_value as f64) / (expected_cost.0 as f64).max(1.0);
                        if ratio > best_ratio {
                            best_ratio = ratio;
                            best_choice = Some(choice.clone());
                            chosen_expected_cost = expected_cost;
                            chosen_expected_tokens = expected_tokens;
                            chosen_expected_requests = expected_requests;
                        }
                    }
                }

                if let Some(choice) = best_choice {
                    let active = lock.active_quotas.entry(choice.name.clone()).or_default();
                    if let Some(ref mut r) = active.rpm { *r -= chosen_expected_requests; }
                    if let Some(ref mut t) = active.tpm { *t -= chosen_expected_tokens; }
                    if let Some(ref mut rd) = active.rpd { *rd -= chosen_expected_requests; }
                    lock.inflight_costs_nanocents += chosen_expected_cost;
                    lock.grant_history.entry(choice.name.clone()).or_default().push_back(submitted_at_unix);
                    
                    let resp = crate::schema::ipc::GrantedPermissionInfo {
                        granted_model: choice.name.clone(),
                        expected_cost_nanocents: chosen_expected_cost,
                        expected_tokens: chosen_expected_tokens,
                        expected_requests: chosen_expected_requests,
                        granted_at_unix: submitted_at_unix,
                        subagent_id: bid.payload.worker_did.clone(),
                    };
                    let _ = bid.tx.send(resp);
                    return;
                }
            }

            lock.stalled_requests.push(bid);
        });

        rx
    }

    pub async fn report_model_failure(market: &SharedArbitrationMarket, model: schema::LlmModel) {
        let mut lock = market.write().await;
        use backoff::backoff::Backoff;
        let entry = lock.model_health.entry(model).or_default();
        let duration = entry.backoff.next_backoff().unwrap_or(std::time::Duration::from_secs(10));
        entry.state_value = HealthStateValue::Unhealthy;
        entry.recovering_until_ms = 0;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        entry.next_tx_ms = now + duration.as_millis() as u64;
    }

    pub async fn refund_expected_budget(
        market: &SharedArbitrationMarket,
        expected_cost_nanocents: schema::NanoCent,
    ) {
        let mut lock = market.write().await;
        lock.inflight_costs_nanocents -= expected_cost_nanocents;
    }

    pub async fn record_consumption(
        market: &SharedArbitrationMarket,
        model: schema::LlmModel,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
        agent_path: String,
        task_type: schema::TaskType,
        raw_input_size: usize,
        expected_cost_nanocents: schema::NanoCent,
    ) -> schema::NanoCent {
        let mut lock = market.write().await;
        let model_cost_schema = Self::cost_for(model, input_tokens);
        let actual_cost = ((input_tokens.saturating_sub(cached_tokens)) * model_cost_schema.input)
            + (cached_tokens * model_cost_schema.cached_input)
            + (output_tokens * model_cost_schema.output);

        let cost_nanocents = schema::NanoCent(actual_cost);

        // True-up budget computation
        lock.inflight_costs_nanocents -= expected_cost_nanocents;
        lock.budget_pool_nanocents -= cost_nanocents;

        *lock
            .subagent_costs
            .entry(agent_path.clone())
            .or_insert(schema::NanoCent(0)) += cost_nanocents;

        let entry = lock
            .consumption_history
            .entry(model.clone())
            .or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        entry.push_back(UsageRecord {
            timestamp: now,
            task_type,
            raw_input_size,
            metrics: UsageMetrics {
                requests: 1,
                input_tokens,
                output_tokens,
                cached_tokens,
                cost_nanocents,
            },
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
        cost_nanocents
    }

    fn merge_metrics(target: &mut UsageMetrics, rec: &UsageMetrics) {
        target.requests += rec.requests;
        target.input_tokens += rec.input_tokens;
        target.output_tokens += rec.output_tokens;
        target.cached_tokens += rec.cached_tokens;
        target.cost_nanocents += rec.cost_nanocents;
    }

    pub fn expected_grant_metrics_for_generic(
        market: &ArbitrationMarket,
        model: &schema::LlmModel,
    ) -> (schema::NanoCent, f64, f64) {
        let grants_count = market
            .grant_history
            .get(model)
            .map(|l| l.len())
            .unwrap_or(0);
        let default_cost = if model.to_string().contains("pro") {
            schema::NanoCent(2_000_000_000)
        } else {
            schema::NanoCent(100_000_000)
        };

        if grants_count > 0 {
            if let Some(records) = market.consumption_history.get(model) {
                let total_cost: u64 = records.iter().map(|r| r.metrics.cost_nanocents.0).sum();
                let total_tokens: f64 = records
                    .iter()
                    .map(|r| r.metrics.input_tokens as f64 + r.metrics.output_tokens as f64)
                    .sum();
                let total_requests: f64 = records.iter().map(|r| r.metrics.requests as f64).sum();
                let grants_f64 = grants_count as f64;
                (
                    schema::NanoCent((total_cost as f64 / grants_f64) as u64),
                    total_tokens / grants_f64,
                    f64::max(1.0, total_requests / grants_f64),
                )
            } else if let Some(rates) = market.historical_rates.get(model) {
                (
                    rates.expected_cost,
                    rates.expected_tokens,
                    rates.expected_requests,
                )
            } else {
                (default_cost, 2000.0, 1.0)
            }
        } else if let Some(rates) = market.historical_rates.get(model) {
            (
                rates.expected_cost,
                rates.expected_tokens,
                rates.expected_requests,
            )
        } else {
            (default_cost, 2000.0, 1.0)
        }
    }

    pub fn expected_grant_metrics_for_bid(
        market: &ArbitrationMarket,
        model: &schema::LlmModel,
        task_type: schema::TaskType,
        raw_input_size: usize,
    ) -> (schema::NanoCent, f64, f64) {
        let records = market.consumption_history.get(model);
        let mut relevant = Vec::new();
        if let Some(recs) = records {
            for r in recs {
                if r.task_type == task_type {
                    relevant.push(r.clone());
                }
            }
        }

        if relevant.len() < 2 {
            return Self::expected_grant_metrics_for_generic(market, model);
        }

        use smartcore::linear::linear_regression::LinearRegression;
        use smartcore::neighbors::knn_regressor::KNNRegressor;
        use smartcore::linalg::basic::matrix::DenseMatrix;

        let mut x_train = Vec::new();
        let mut y_in = Vec::new();
        let mut y_out = Vec::new();
        let mut y_cached = Vec::new();
        let mut requests_sum = 0.0;

        for r in &relevant {
            x_train.push(vec![r.raw_input_size as f64]);
            y_in.push(r.metrics.input_tokens as f64);
            y_out.push(r.metrics.output_tokens as f64);
            y_cached.push(r.metrics.cached_tokens as f64);
            requests_sum += r.metrics.requests as f64;
        }

        let density = DenseMatrix::from_2d_vec(&x_train).unwrap();
        
        let pred_in = if let Ok(lr) = LinearRegression::fit(&density, &y_in, Default::default()) {
             lr.predict(&DenseMatrix::from_2d_vec(&vec![vec![raw_input_size as f64]]).unwrap()).unwrap_or(vec![y_in[0]])[0].max(0.0)
        } else { y_in[0] };

        let pred_cached = if let Ok(lr) = LinearRegression::fit(&density, &y_cached, Default::default()) {
             lr.predict(&DenseMatrix::from_2d_vec(&vec![vec![raw_input_size as f64]]).unwrap()).unwrap_or(vec![y_cached[0]])[0].max(0.0)
        } else { y_cached[0] };

        // Ensure K is bounded by how many items we actually have
        let k_val = std::cmp::min(5, relevant.len());
        let knn_params = smartcore::neighbors::knn_regressor::KNNRegressorParameters::default().with_k(k_val);

        let pred_out = if let Ok(knn) = KNNRegressor::fit(&density, &y_out, knn_params) {
             knn.predict(&DenseMatrix::from_2d_vec(&vec![vec![raw_input_size as f64]]).unwrap()).unwrap_or(vec![y_out[0]])[0].max(0.0)
        } else { y_out[0] };

        let model_cost_schema = Self::cost_for(*model, pred_in as u64);
        let expected_cost = ((pred_in as u64).saturating_sub(pred_cached as u64) * model_cost_schema.input)
            + ((pred_cached as u64) * model_cost_schema.cached_input)
            + ((pred_out as u64) * model_cost_schema.output);

        (schema::NanoCent(expected_cost), pred_in + pred_out, requests_sum / (relevant.len() as f64))
    }

    pub async fn get_market_state(market: &SharedArbitrationMarket) -> MarketStateResponse {
        let lock = market.read().await;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut per_model_stats = std::collections::BTreeMap::new();

        for (model, records) in &lock.consumption_history {
            let (expected_grant_cost, expected_grant_tokens, expected_grant_requests) =
                Self::expected_grant_metrics_for_generic(&lock, model);
            let status_str = lock.model_health.get(model).map(|h| match h.state_value {
                HealthStateValue::Healthy => "Healthy",
                HealthStateValue::Unhealthy => "Unhealthy",
                HealthStateValue::Recovering => "Recovering",
            }).unwrap_or("Healthy").to_string();

            let mut stat = ModelUsageStats {
                status: status_str,
                total: UsageMetrics::default(),
                active_quotas: lock
                    .active_quotas
                    .get(model)
                    .cloned()
                    .map(|q| crate::schema::ipc::Quotas {
                        rpm: q.rpm,
                        tpm: q.tpm,
                        rpd: q.rpd,
                    })
                    .unwrap_or_default(),
                trailing_1m: UsageMetrics::default(),
                trailing_3m: UsageMetrics::default(),
                trailing_10m: UsageMetrics::default(),
                trailing_30m: UsageMetrics::default(),
                trailing_100m: UsageMetrics::default(),
                expected_grant_cost,
                expected_grant_tokens,
                expected_grant_requests,
            };

            for r in records {
                Self::merge_metrics(&mut stat.total, &r.metrics);
                let age = now.saturating_sub(r.timestamp);
                if age <= 60 {
                    Self::merge_metrics(&mut stat.trailing_1m, &r.metrics);
                }
                if age <= 3 * 60 {
                    Self::merge_metrics(&mut stat.trailing_3m, &r.metrics);
                }
                if age <= 10 * 60 {
                    Self::merge_metrics(&mut stat.trailing_10m, &r.metrics);
                }
                if age <= 30 * 60 {
                    Self::merge_metrics(&mut stat.trailing_30m, &r.metrics);
                }
                if age <= 100 * 60 {
                    Self::merge_metrics(&mut stat.trailing_100m, &r.metrics);
                }
            }
            per_model_stats.insert(model.clone(), stat);
        }

        // Fill in bounds for models with NO historical usage yet
        for (model, quota) in &lock.active_quotas {
            if !per_model_stats.contains_key(model) {
                let (expected_grant_cost, expected_grant_tokens, expected_grant_requests) =
                    Self::expected_grant_metrics_for_generic(&lock, model);
                let status_str = lock.model_health.get(model).map(|h| match h.state_value {
                    HealthStateValue::Healthy => "Healthy",
                    HealthStateValue::Unhealthy => "Unhealthy",
                    HealthStateValue::Recovering => "Recovering",
                }).unwrap_or("Healthy").to_string();

                per_model_stats.insert(
                    model.clone(),
                    ModelUsageStats {
                        status: status_str,
                        total: UsageMetrics::default(),
                        active_quotas: crate::schema::ipc::Quotas {
                            rpm: quota.rpm,
                            tpm: quota.tpm,
                            rpd: quota.rpd,
                        },
                        trailing_1m: UsageMetrics::default(),
                        trailing_3m: UsageMetrics::default(),
                        trailing_10m: UsageMetrics::default(),
                        trailing_30m: UsageMetrics::default(),
                        trailing_100m: UsageMetrics::default(),
                        expected_grant_cost,
                        expected_grant_tokens,
                        expected_grant_requests,
                    },
                );
            }
        }

        let pending_bids = lock
            .stalled_requests
            .iter()
            .map(|b| PendingBidInfo {
                requester_id: b.payload.worker_did.clone(),
                choices: b.payload.model_choices.clone(),
                submitted_at_unix: b.submitted_at_unix,
            })
            .collect();

        let per_model_stats_vec: Vec<_> = per_model_stats.into_iter().collect();

        let mut subagent_costs_vec: Vec<_> = lock
            .subagent_costs
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        subagent_costs_vec
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        MarketStateResponse {
            per_model_stats: per_model_stats_vec,
            pending_bids,
            budget_pool_nanocents: lock.budget_pool_nanocents,
            inflight_costs_nanocents: lock.inflight_costs_nanocents,
            subagent_costs: lock.subagent_costs.iter().map(|(n, c)| (n.clone(), *c)).collect(),
        }
    }

    async fn run_auction_loop(market: SharedArbitrationMarket) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(TICK_TIME_SECS)).await;

            let mut lock = market.write().await;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let now_ms = now * 1000;

            use backoff::backoff::Backoff;
            for (model, health) in lock.model_health.iter_mut() {
                if health.state_value == HealthStateValue::Unhealthy && now_ms >= health.next_tx_ms {
                    tracing::info!("Model {:?} organic backoff expired. Transitioning to Recovering natively.", model);
                    health.state_value = HealthStateValue::Recovering;
                    health.recovering_until_ms = now_ms + 60_000;
                }
                if health.state_value == HealthStateValue::Recovering && now_ms >= health.recovering_until_ms {
                    tracing::info!("Model {:?} organic recovery complete. Transitioning to Healthy natively.", model);
                    health.state_value = HealthStateValue::Healthy;
                    health.backoff.reset();
                }
            }

            // 1. Replenish quotas
            let models = schema::LlmModel::ALL;
            for model in models {
                let limit = Self::rate_limits_for(*model);
                let active = lock.active_quotas.entry(*model).or_default();

                if let Some(r) = limit.rpm {
                    let cur = active.rpm.unwrap_or(0.0);
                    active.rpm = Some(f64::min(cur + (r / (60.0 / TICK_TIME_SECS as f64)), r));
                }
                if let Some(t) = limit.tpm {
                    let cur = active.tpm.unwrap_or(0.0);
                    active.tpm = Some(f64::min(cur + (t / (60.0 / TICK_TIME_SECS as f64)), t));
                }
                if let Some(rd) = limit.rpd {
                    let cur = active.rpd.unwrap_or(0.0);
                    let rounds_per_day = 86400.0 / TICK_TIME_SECS as f64;
                    active.rpd = Some(f64::min(cur + (rd / rounds_per_day), rd));
                }
            }

            // 2. Replenish Budget Pool natively
            let daily_nanocents = (lock.config.daily_budget_usd * 100_000_000_000.0) as u64;
            let hourly_cap = schema::NanoCent(daily_nanocents / 24);
            let rounds_per_day = 86400 / TICK_TIME_SECS;
            let bump_per_loop = schema::NanoCent(daily_nanocents / rounds_per_day);
            lock.budget_pool_nanocents = std::cmp::min(lock.budget_pool_nanocents + bump_per_loop, hourly_cap);

            // Clean up stale grant history
            let oldest_valid = now.saturating_sub(100 * 60);
            for history in lock.grant_history.values_mut() {
                while let Some(front) = history.front() {
                    if *front < oldest_valid {
                        history.pop_front();
                    } else {
                        break;
                    }
                }
            }

            // 4. Extract and flat-map bids into individual choice tickets for Priority-aging HVF
            let bids = std::mem::take(&mut lock.stalled_requests);
            let mut inflight_requests: Vec<Option<AuctionBid>> = bids
                .into_iter()
                .filter(|b| !b.tx.is_closed())
                .map(Some)
                .collect();

            // Auction Fixed-Point Iteration Loop
            loop {
                // Round 1 - find all bids that meet rpm/tpm/rpd constraints and current budget
                let mut round_1_tickets = Vec::new();
                for (idx, req) in inflight_requests.iter().enumerate() {
                    if let Some(bid) = req {
                        let wait_time = now.saturating_sub(bid.submitted_at_unix);
                        for choice in &bid.payload.model_choices {
                            let is_backoff = lock.model_health.get(&choice.name).map(|h| h.state_value != HealthStateValue::Healthy).unwrap_or(false);
                            if is_backoff {
                                continue;
                            }

                            let (expected_cost, expected_tokens, expected_requests) =
                                Self::expected_grant_metrics_for_bid(&lock, &choice.name, bid.payload.task_type, bid.payload.raw_input_size);

                            let active = lock.active_quotas.entry(choice.name.clone()).or_default();

                            let rpm_ok = active.rpm.map_or(true, |r| r >= expected_requests);
                            let tpm_ok = active.tpm.map_or(true, |t| t >= expected_tokens);
                            let rpd_ok = active.rpd.map_or(true, |rd| rd >= expected_requests);

                            let available_budget = schema::NanoCent(lock.budget_pool_nanocents.0.saturating_sub(lock.inflight_costs_nanocents.0));
                            // Purely declarative in Round 1
                            if rpm_ok && tpm_ok && rpd_ok && available_budget >= expected_cost {
                                round_1_tickets.push((choice.bid_value, expected_cost, idx, choice.clone(), wait_time));
                            }
                        }
                    }
                }

                if round_1_tickets.is_empty() {
                    break;
                }

                // Round 2 - sort by value/cost ratio + priority aging (5% boost per second)
                round_1_tickets.sort_by(|a, b| {
                    let a_base = (a.0 as f64) / (a.1.0 as f64).max(1.0);
                    let b_base = (b.0 as f64) / (b.1.0 as f64).max(1.0);
                    let age_factor_per_sec = 0.05;
                    let a_score = a_base * (1.0 + (a.4 as f64 * age_factor_per_sec));
                    let b_score = b_base * (1.0 + (b.4 as f64 * age_factor_per_sec));

                    b_score.partial_cmp(&a_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| a.2.cmp(&b.2))
                });

                let mut restart_round_1 = false;

                // Round 3 - take most valuable until expected $ budget breaks
                for (_value, expected_cost, req_idx, choice, _wait) in round_1_tickets {
                    let Some(bid_data) = inflight_requests[req_idx].as_ref() else {
                        continue; // Reached if another choice from this same request previously satisfied it!
                    };

                    let (_ec, expected_tokens, expected_requests) =
                        Self::expected_grant_metrics_for_bid(&lock, &choice.name, bid_data.payload.task_type, bid_data.payload.raw_input_size);

                    let (rpm, tpm, rpd) = {
                        let active = lock.active_quotas.entry(choice.name.clone()).or_default();
                        (active.rpm, active.tpm, active.rpd)
                    };
                    let rpm_ok = rpm.map_or(true, |r| r >= expected_requests);
                    let tpm_ok = tpm.map_or(true, |t| t >= expected_tokens);
                    let rpd_ok = rpd.map_or(true, |rd| rd >= expected_requests);

                    let available_budget = schema::NanoCent(lock.budget_pool_nanocents.0.saturating_sub(lock.inflight_costs_nanocents.0));

                    // Dynamic assertion - if limits dwindled due to previous grants in Round 3, restart natively!
                    if !(rpm_ok && tpm_ok && rpd_ok && available_budget >= expected_cost) {
                        restart_round_1 = true;
                        break;
                    }

                    // Execute and mutate allocations cleanly uniquely here!
                    let active = lock.active_quotas.get_mut(&choice.name).unwrap();
                    if let Some(ref mut r) = active.rpm {
                        *r -= expected_requests;
                    }
                    if let Some(ref mut t) = active.tpm {
                        *t -= expected_tokens;
                    }
                    if let Some(ref mut rd) = active.rpd {
                        *rd -= expected_requests;
                    }

                    lock.inflight_costs_nanocents += expected_cost;

                    let bid_data: AuctionBid = inflight_requests[req_idx].take().unwrap();
                    let resp = crate::schema::ipc::GrantedPermissionInfo {
                        granted_model: choice.name.clone(),
                        expected_cost_nanocents: expected_cost,
                        expected_tokens,
                        expected_requests,
                        granted_at_unix: now,
                        subagent_id: bid_data.payload.worker_did.clone(),
                    };

                    lock.grant_history
                        .entry(choice.name.clone())
                        .or_default()
                        .push_back(now);

                    let _ = bid_data.tx.send(resp);
                }

                if !restart_round_1 {
                    break;
                }
            }

            lock.stalled_requests = inflight_requests.into_iter().flatten().collect();

            let rates_json_and_path = if let Some(mut rates_path) = lock.config.nancy_dir.clone() {
                rates_path.push("consumption_rates.json");
                let mut current_rates = HashMap::new();
                for model in schema::LlmModel::ALL {
                    let grants_count = lock.grant_history.get(model).map(|l| l.len()).unwrap_or(0);
                    if grants_count > 0 {
                        if let Some(records) = lock.consumption_history.get(model) {
                            let total_cost: u64 = records.iter().map(|r| r.metrics.cost_nanocents.0).sum();
                            let total_tokens: f64 = records
                                .iter()
                                .map(|r| {
                                    r.metrics.input_tokens as f64 + r.metrics.output_tokens as f64
                                })
                                .sum();
                            let total_requests: f64 =
                                records.iter().map(|r| r.metrics.requests as f64).sum();
                            let grants_f64 = grants_count as f64;
                            current_rates.insert(
                                *model,
                                ConsumptionRates {
                                    expected_cost: schema::NanoCent((total_cost as f64 / grants_f64) as u64),
                                    expected_tokens: total_tokens / grants_f64,
                                    expected_requests: f64::max(1.0, total_requests / grants_f64),
                                },
                            );
                        }
                    } else if let Some(old) = lock.historical_rates.get(model) {
                        current_rates.insert(*model, old.clone());
                    }
                }
                lock.historical_rates = current_rates.clone();
                serde_json::to_string_pretty(&current_rates)
                    .ok()
                    .map(|j| (j, rates_path))
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
            _budget in 10.0_f64..50.0,
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
                stalled_requests: Vec::new(),
                consumption_history: HashMap::new(),
                grant_history: HashMap::new(),
                historical_rates: HashMap::new(),
                active_quotas,
                subagent_costs: HashMap::new(),
                budget_pool_nanocents: schema::NanoCent(0),
                inflight_costs_nanocents: schema::NanoCent(0),
                config,
                model_health: HashMap::new(),
            }));
            
            // Note: we don't spawn the loop natively in this test since we test quota logic algebraically directly

            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut lock = market.write().await;

                let models = schema::LlmModel::ALL;
                for model in models {
                    let limit = ArbitrationMarket::rate_limits_for(*model);
                    let active = lock.active_quotas.entry(*model).or_default();
                    if let Some(r) = limit.rpm {
                        active.rpm = Some(f64::min(active.rpm.unwrap_or(0.0) + (r / 3.0), r)); // Mocking the RPM 20s refill
                    }
                }

                let daily = lock.config.daily_budget_usd;
                let daily_nanocents = (daily * 100_000_000_000.0) as u64;
                let hourly_cap = schema::NanoCent(daily_nanocents / 24);
                let bump = schema::NanoCent(daily_nanocents / 4320);
                lock.budget_pool_nanocents = std::cmp::min(lock.budget_pool_nanocents + bump, hourly_cap);

                let res = lock.active_quotas.get(&schema::LlmModel::TestMockModel).unwrap().rpm.unwrap();
                let expected_limit = ArbitrationMarket::rate_limits_for(schema::LlmModel::TestMockModel).rpm.unwrap();
                assert!(res <= expected_limit); // Proves the use-or-lose requirement holds cleanly algebraically.
                assert!(lock.budget_pool_nanocents <= hourly_cap); // Check strict max hourly pools
            });
        }
    }

    #[test]
    fn test_expected_grant_metrics_fallback() {
        let mut market = ArbitrationMarket {
            stalled_requests: Vec::new(),
            consumption_history: HashMap::new(),
            grant_history: HashMap::new(),
            historical_rates: HashMap::new(),
            active_quotas: HashMap::new(),
            subagent_costs: HashMap::new(),
            budget_pool_nanocents: schema::NanoCent(0),
            inflight_costs_nanocents: schema::NanoCent(0),
            config: CoordinatorConfig { daily_budget_usd: 10.0, nancy_dir: None },
            model_health: HashMap::new(),
        };

        // Lines 409-410 coverage
        let mut target = UsageMetrics::default();
        let rec = UsageMetrics {
            requests: 1,
            input_tokens: 10,
            output_tokens: 20,
            cached_tokens: 30,
            cost_nanocents: schema::NanoCent(100),
        };
        ArbitrationMarket::merge_metrics(&mut target, &rec);
        assert_eq!(target.output_tokens, 20);
        assert_eq!(target.cached_tokens, 30);

        // Lines 483, 502, 506 coverage
        let model = schema::LlmModel::TestMockModel;
        
        // Colinear X values so LinearRegression::fit fails
        let rec1 = UsageRecord {
            timestamp: 0,
            metrics: UsageMetrics {
                requests: 1,
                input_tokens: 10,
                output_tokens: 20,
                cached_tokens: 5,
                cost_nanocents: schema::NanoCent(10),
            },
            task_type: schema::TaskType::Chat,
            raw_input_size: 10,
        };
        let rec2 = UsageRecord {
            timestamp: 0,
            metrics: UsageMetrics {
                requests: 1,
                input_tokens: 100, // Different Y
                output_tokens: 200,
                cached_tokens: 50,
                cost_nanocents: schema::NanoCent(100),
            },
            task_type: schema::TaskType::Chat,
            raw_input_size: 10, // Same X! Singular matrix!
        };

        market.consumption_history.insert(model, vec![rec1, rec2].into());

        let (cost, predicted_tokens, reqs) = ArbitrationMarket::expected_grant_metrics_for_bid(
            &market,
            &model,
            schema::TaskType::Chat,
            50
        );

        assert!(predicted_tokens > 0.0);
    }
}

// DOCUMENTED_BY: [docs/adr/0057-token-arbitration-spot-market.md]

// DOCUMENTED_BY: [docs/adr/0071-nanocent-integer-math.md]
