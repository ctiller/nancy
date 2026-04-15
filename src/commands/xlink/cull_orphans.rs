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
    let mut all_files = std::collections::HashSet::new();

    for path_str in &tracked_files {
        all_files.insert(path_str.clone());
    }

    for path_str in &tracked_files {
        let full_path = cwd.join(path_str);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let data = extract_tags(&content);
            
            let check_and_cull = |links: &Vec<String>, tag_name: &str| -> Result<()> {
                for link in links {
                    if link == "none" { continue; }
                    if !all_files.contains(link) {
                        println!("Culling orphan link {} -> {} ({})", path_str, link, tag_name);
                        remove_tag(&full_path, tag_name, link)?;
                    }
                }
                Ok(())
            };

            check_and_cull(&data.implemented_by, "IMPLEMENTED_BY")?;
            check_and_cull(&data.documented_by, "DOCUMENTED_BY")?;
            check_and_cull(&data.tested_by, "TESTED_BY")?;
            check_and_cull(&data.deprecates, "DEPRECATES")?;
            check_and_cull(&data.deprecated_by, "DEPRECATED_BY")?;
            check_and_cull(&data.see_also, "SEE_ALSO")?;
        }
    }

    println!("Successfully culled orphan codebase xlinks.");
    Ok(())
}
