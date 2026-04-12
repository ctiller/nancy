#[cfg(test)]
mod tests {
    use serde::{Serialize, Deserialize};

    #[derive(Serialize, Deserialize, Debug)]
    struct Mock {
        val: Option<f64>,
    }

    #[test]
    fn test_inf() {
        let m = Mock { val: Some(f64::INFINITY) };
        let js = match serde_json::to_string(&m) {
            Ok(js) => js,
            Err(e) => format!("ERR: {}", e),
        };
        println!("JSON: {}", js);
    }
}
