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
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default)]
struct XlinkData {
    implemented_by: Vec<String>,
    documented_by: Vec<String>,
    tested_by: Vec<String>,
    deprecates: Vec<String>,
    deprecated_by: Vec<String>,
    see_also: Vec<String>,
    unimplemented: Option<String>,
}

fn extract_tags(content: &str) -> XlinkData {
    let mut data = XlinkData::default();

    // Regex to match TAG: [ ... ] spanning multiple lines potentially
    let re = Regex::new(r"(?s)(IMPLEMENTED_BY|DOCUMENTED_BY|TESTED_BY|DEPRECATES|DEPRECATED_BY|SEE_ALSO):\s*\[(.*?)\]").unwrap();

    let clean_re = Regex::new(r"(//|#|<!--|-->|\*|/\*|\*/)").unwrap();

    for cap in re.captures_iter(content) {
        let tag = cap.get(1).map_or("", |m| m.as_str());
        let raw_list = cap.get(2).map_or("", |m| m.as_str());

        let cleaned = clean_re.replace_all(raw_list, "");
        let paths: Vec<String> = cleaned
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        match tag {
            "IMPLEMENTED_BY" => data.implemented_by.extend(paths),
            "DOCUMENTED_BY" => data.documented_by.extend(paths),
            "TESTED_BY" => data.tested_by.extend(paths),
            "DEPRECATES" => data.deprecates.extend(paths),
            "DEPRECATED_BY" => data.deprecated_by.extend(paths),
            "SEE_ALSO" => data.see_also.extend(paths),
            _ => {}
        }
    }

    let unimp_re = Regex::new(r#"UNIMPLEMENTED:\s*"(.*?)""#).unwrap();
    if let Some(cap) = unimp_re.captures(content) {
        data.unimplemented = Some(cap.get(1).unwrap().as_str().to_string());
    }

    data
}

pub async fn run(cwd: PathBuf) -> Result<()> {
    let mut errors = Vec::new();
    let mut all_files = HashMap::new();

    // Scan all git tracked files (approximated here by walking and avoiding .git and target for simplicity)
    let repo = git2::Repository::discover(&cwd)?;
    let index = repo.index()?;
    let workdir = repo.workdir().unwrap_or(&cwd);

    let prefix = workdir.to_string_lossy().to_string();

    let mut tracked_files = Vec::new();
    for entry in index.iter() {
        let path_str = String::from_utf8(entry.path).unwrap();
        if path_str.starts_with("3p/") || path_str == "3p" || path_str.starts_with(".agents/") || path_str == ".agents" {
            continue;
        }
        let path = workdir.join(&path_str);
        if path.is_file() {
            tracked_files.push(path_str);
        }
    }

    let pos_re = Regex::new(r"(?s)(IMPLEMENTED_BY|DOCUMENTED_BY|TESTED_BY|DEPRECATES|DEPRECATED_BY|SEE_ALSO):\s*\[.*?\]").unwrap();

    for path_str in &tracked_files {
        let full_path = workdir.join(path_str);
        if let Ok(content) = fs::read_to_string(&full_path) {
            let data = extract_tags(&content);
            all_files.insert(path_str.clone(), data);

            // Check if tags are at the end of file
            let mut last_end = 0;
            for cap in pos_re.find_iter(&content) {
                last_end = cap.end();
            }
            if last_end > 0 {
                let remainder = &content[last_end..];
                let cleaned = remainder.replace("-->", "").replace("*/", "");
                if !cleaned.trim().is_empty() {
                    errors.push(format!("File {} xlink tags are not at the end of the file", path_str));
                }
            }
        }
    }

    // Rules verification
    for (file, data) in &all_files {
        let is_persona = file.contains("src/personas/");
        let is_doc = !is_persona && (file.ends_with(".md") || file.ends_with(".txt") || file.contains("docs/"));
        let is_source = is_persona || (!is_doc && (file.ends_with(".rs") || file.ends_with(".js") || file.ends_with(".html")));

        if is_doc {
            if data.implemented_by.is_empty() && data.unimplemented.is_none() {
                errors.push(format!("Documentation {} missing IMPLEMENTED_BY or UNIMPLEMENTED tag", file));
            }
            if data.implemented_by.contains(&"none".to_string()) {
                errors.push(format!("Documentation {} IMPLEMENTED_BY tag cannot be 'none'", file));
            }
        }

        if is_source {
            if data.documented_by.is_empty() || data.documented_by.contains(&"none".to_string()) {
                errors.push(format!("Source {} must have a valid DOCUMENTED_BY tag (no 'none')", file));
            }

            if data.tested_by.contains(&"none".to_string()) {
                errors.push(format!("Source {} TESTED_BY tag cannot be 'none'", file));
            }
        }

        // Check existence of relative paths and bidirectionality
        let check_links = |links: &Vec<String>, tag_name: &str, opposite_tag: &str| {
            let mut file_errs = Vec::new();
            for link in links {
                if link == "none" {
                    continue;
                }
                if link.starts_with("3p/") || link == "3p" {
                    file_errs.push(format!("File {} cannot reference 3p/ file {}", file, link));
                    continue;
                }
                if !all_files.contains_key(link) {
                    file_errs.push(format!("File {} references missing {} ({})", file, link, tag_name));
                } else {
                    // Check bidirectionality
                    let target_data = all_files.get(link).unwrap();
                    let target_links = match opposite_tag {
                        "IMPLEMENTED_BY" => &target_data.implemented_by,
                        "DOCUMENTED_BY" => &target_data.documented_by,
                        "TESTED_BY" => &target_data.tested_by,
                        "DEPRECATES" => &target_data.deprecates,
                        "DEPRECATED_BY" => &target_data.deprecated_by,
                        "SEE_ALSO" => &target_data.see_also,
                        _ => { panic!("Unknown tag") }
                    };
                    if !target_links.contains(file) {
                        file_errs.push(format!("Unilateral link: {} points to {} via {} but target does not specify {}", file, link, tag_name, opposite_tag));
                    }
                }
            }
            file_errs
        };

        errors.extend(check_links(&data.implemented_by, "IMPLEMENTED_BY", "DOCUMENTED_BY"));
        // For tests, test -> IMPLEMENTED_BY -> source. Source -> TESTED_BY -> test.
        // Wait, tests would have IMPLEMENTED_BY linking to source.
        errors.extend(check_links(&data.documented_by, "DOCUMENTED_BY", "IMPLEMENTED_BY"));
        errors.extend(check_links(&data.tested_by, "TESTED_BY", "IMPLEMENTED_BY"));
        errors.extend(check_links(&data.deprecates, "DEPRECATES", "DEPRECATED_BY"));
        errors.extend(check_links(&data.deprecated_by, "DEPRECATED_BY", "DEPRECATES"));
        errors.extend(check_links(&data.see_also, "SEE_ALSO", "SEE_ALSO"));
    }

    if !errors.is_empty() {
        for err in &errors {
            println!("{}", err);
        }
        anyhow::bail!("xlink audit failed with {} errors", errors.len());
    }

    println!("All xlink tags successfully audited bidirectionally.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tags() {
        let content = format!(r#"
        /*
        {}: [
            foo/bar.md,
            baz.md
        ]
        {}: [none]
        */
        "#, "DOCUMENTED_BY", "IMPLEMENTED_BY");
        let data = extract_tags(content);
        assert_eq!(data.documented_by, vec!["foo/bar.md", "baz.md"]);
        assert_eq!(data.implemented_by, vec!["none"]);
    }
}

// DOCUMENTED_BY: [docs/adr/0076-xlink-microformat.md]
