#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    
    #[test]
    fn test_market_state_parsing() {
        let mut map: BTreeMap<schema::LlmModel, schema::ModelUsageStats> = BTreeMap::new();
        map.insert(
            schema::LlmModel::TestMockModel, 
            schema::ModelUsageStats {
                total: schema::UsageMetrics::default(),
                active_quotas: schema::Quotas::default(),
                trailing_1m: schema::UsageMetrics::default(),
                trailing_3m: schema::UsageMetrics::default(),
                trailing_10m: schema::UsageMetrics::default(),
                trailing_30m: schema::UsageMetrics::default(),
                trailing_100m: schema::UsageMetrics::default(),
            }
        );
        let resp = schema::MarketStateResponse {
            per_model_stats: map,
            pending_bids: vec![],
            active_leases: vec![]
        };

        let js = serde_json::to_string(&resp).unwrap();
        println!("JSON MAP: {}", js);
        
        let parsed: Result<schema::MarketStateResponse, _> = serde_json::from_str(&js);
        if let Err(e) = parsed {
            panic!("PARSE FAILED: {}", e);
        } else {
            println!("SUCCESS!");
        }
    }
}
