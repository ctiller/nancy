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
use regex::Regex;

pub async fn run(cwd: PathBuf) -> Result<()> {
    let tracked_files = get_tracked_files(&cwd)?;

    let tag_re = Regex::new(r"(IMPLEMENTED_BY|DOCUMENTED_BY|TESTED_BY|DEPRECATES|DEPRECATED_BY|SEE_ALSO):").unwrap();

    for path_str in &tracked_files {
        let full_path = cwd.join(path_str);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let data = extract_tags(&content);
            
            // Check if already at the end
            let mut last_end = 0;
            let full_re = Regex::new(r"(?s)(IMPLEMENTED_BY|DOCUMENTED_BY|TESTED_BY|DEPRECATES|DEPRECATED_BY|SEE_ALSO):\s*\[.*?\]").unwrap();
            for cap in full_re.find_iter(&content) {
                last_end = cap.end();
            }
            let is_at_end = if last_end > 0 {
                let remainder = &content[last_end..];
                let cleaned = remainder.replace("-->", "").replace("*/", "");
                cleaned.trim().is_empty()
            } else {
                true // No tags is technically at the end
            };

            if !is_at_end {
                println!("Fixing tag position for {}", path_str);
                
                // Remove tag lines
                let lines: Vec<&str> = content.lines().filter(|line| !tag_re.is_match(line)).collect();
                let mut new_content = lines.join("\n");
                
                // Ensure trailing newline
                if !new_content.ends_with("\n") && !new_content.is_empty() {
                    new_content.push('\n');
                }
                
                fs::write(&full_path, new_content)?;

                let is_rust = path_str.ends_with(".rs");

                // Re-add tags using append_tag
                let mut add_tags = |links: &Vec<String>, tag_name: &str| -> Result<()> {
                    for link in links {
                        append_tag(&full_path, tag_name, link, is_rust)?;
                    }
                    Ok(())
                };

                add_tags(&data.implemented_by, "IMPLEMENTED_BY")?;
                add_tags(&data.documented_by, "DOCUMENTED_BY")?;
                add_tags(&data.tested_by, "TESTED_BY")?;
                add_tags(&data.deprecates, "DEPRECATES")?;
                add_tags(&data.deprecated_by, "DEPRECATED_BY")?;
                add_tags(&data.see_also, "SEE_ALSO")?;
            }
        }
    }

    println!("Successfully corrected xlink positions.");
    Ok(())
}

// DOCUMENTED_BY: [docs/adr/0076-xlink-microformat.md]
