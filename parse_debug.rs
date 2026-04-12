use nancy::coordinator::market::ArbitrationMarket;
use nancy::schema::coordinator_config::CoordinatorConfig;

#[tokio::main]
async fn main() {
    let market = ArbitrationMarket::new(CoordinatorConfig::default());
    let state = ArbitrationMarket::get_market_state(&market).await;
    let json_text = serde_json::to_string(&state).unwrap();
    println!("Serialized: {}", json_text);
    
    // Now try to parse using schema::MarketStateResponse
    match serde_json::from_str::<nancy::schema::MarketStateResponse>(&json_text) {
        Ok(_) => println!("Parse OK!"),
        Err(e) => println!("Parse Error: {}", e),
    }
}
