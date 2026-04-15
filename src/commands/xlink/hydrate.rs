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

use anyhow::Result;
use std::path::PathBuf;
use std::fs;
use crate::commands::xlink::common::*;

pub async fn run(cwd: PathBuf) -> Result<()> {
    let tracked_files = get_tracked_files(&cwd)?;
    let mut all_files = std::collections::HashMap::new();

    for path_str in &tracked_files {
        let full_path = cwd.join(path_str);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let data = extract_tags(&content);
            all_files.insert(path_str.clone(), (full_path, data));
        }
    }

    for (file, (full_path, data)) in &all_files {
        // 1. If I am IMPLEMENTED_BY [source], then [source] should be DOCUMENTED_BY [me]
        for source in &data.implemented_by {
            if source == "none" { continue; }
            if let Some((target_full_path, _)) = all_files.get(source) {
                let is_rust = source.ends_with(".rs");
                append_tag(target_full_path, "DOCUMENTED_BY", file, is_rust)?;
            }
        }

        // 2. If I am DOCUMENTED_BY [doc], then [doc] should be IMPLEMENTED_BY [me]
        for doc in &data.documented_by {
            if doc == "none" { continue; }
            if let Some((target_full_path, _)) = all_files.get(doc) {
                let is_rust = doc.ends_with(".rs");
                append_tag(target_full_path, "IMPLEMENTED_BY", file, is_rust)?;
            }
        }
    }

    println!("Successfully hydrated codebase xlinks natively.");
    Ok(())
}
