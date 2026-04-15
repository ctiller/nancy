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

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

pub async fn debug_eval(file_path: PathBuf) -> Result<()> {
    let file = std::fs::File::open(&file_path)
        .with_context(|| format!("Failed to open file {:?}", file_path))?;
    
    let data: serde_yaml::Value = serde_yaml::from_reader(file)
        .with_context(|| format!("Failed to parse YAML from {:?}", file_path))?;

    let mut tool_calls: HashMap<String, Vec<String>> = HashMap::new();
    let mut loops_detected = 0;

    if let Some(traces) = data.get("traces").and_then(|t| t.as_sequence()) {
        for item in traces {
            if let Some(ttype) = item.get("$type").and_then(|t| t.as_str()) {
                if ttype == "llm_tool_call" {
                    if let Some(fn_name) = item.get("function_name").and_then(|n| n.as_str()) {
                        let args = item.get("args").map(|a| {
                            serde_json::to_string(a).unwrap_or_default()
                        }).unwrap_or_default();
                        tool_calls.entry(fn_name.to_string()).or_default().push(args);
                    }
                } else if ttype == "llm_response" {
                    if let Some(resp) = item.get("response").and_then(|r| r.as_str()) {
                        if resp.contains(r#""is_looping": true"#) || resp.contains(r#""is_looping":true"#) {
                            loops_detected += 1;
                        }
                    } else if let Some(resp) = item.get("response").and_then(|r| r.as_mapping()) {
                        if let Some(is_looping) = resp.get("is_looping") {
                            if is_looping.as_bool() == Some(true) {
                                loops_detected += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    println!("--- Tool Calls ---");
    let mut fns: Vec<_> = tool_calls.keys().collect();
    fns.sort();
    
    for fn_name in fns {
        let calls = &tool_calls[fn_name];
        println!("Function: {} (Count: {})", fn_name, calls.len());
        
        let mut counter: HashMap<&String, usize> = HashMap::new();
        for call in calls {
            *counter.entry(call).or_insert(0) += 1;
        }

        let mut counts: Vec<(&&String, &usize)> = counter.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

        for (arg, freq) in counts.iter().take(5) {
            println!("  {}x : {}", freq, arg);
        }
    }

    println!("--- Loop Detectors Fired: {} ---", loops_detected);

    Ok(())
}
