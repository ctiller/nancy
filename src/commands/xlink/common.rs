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
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Default, Clone)]
pub struct XlinkData {
    pub implemented_by: Vec<String>,
    pub documented_by: Vec<String>,
    pub tested_by: Vec<String>,
    pub deprecates: Vec<String>,
    pub deprecated_by: Vec<String>,
    pub see_also: Vec<String>,
}

pub fn extract_tags(content: &str) -> XlinkData {
    let mut data = XlinkData::default();
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
    data
}

pub fn get_tracked_files(cwd: &PathBuf) -> Result<Vec<String>> {
    let repo = git2::Repository::discover(cwd)?;
    let index = repo.index()?;
    let workdir = repo.workdir().unwrap_or(cwd);

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
    Ok(tracked_files)
}

pub fn append_tag(path: &PathBuf, tag: &str, target: &str, is_rust: bool) -> Result<()> {
    let mut content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let re = Regex::new(&format!(r"({}:\s*\[)(.*?)(\])", tag)).unwrap();
    
    if let Some(caps) = re.captures(&content) {
        let inside = caps.get(2).unwrap().as_str();
        if !inside.contains(target) {
            let mut parts: Vec<&str> = inside.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if parts.contains(&"none") {
                parts.retain(|&x| x != "none");
            }
            parts.push(target);
            let new_inside = parts.join(", ");
            let new_tag = format!("{}{}{}", caps.get(1).unwrap().as_str(), new_inside, caps.get(3).unwrap().as_str());
            content = content.replace(caps.get(0).unwrap().as_str(), &new_tag);
            fs::write(path, content)?;
        }
    } else {
        let new_line = if is_rust {
            format!("// {}: [{}]\n", tag, target)
        } else {
            format!("<!-- {}: [{}] -->\n", tag, target)
        };
        
        content.push_str("\n");
        content.push_str(&new_line);
        fs::write(path, content)?;
    }
    Ok(())
}

pub fn remove_tag(path: &PathBuf, tag: &str, target: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut content = fs::read_to_string(path)?;
    let re = Regex::new(&format!(r"({}:\s*\[)(.*?)(\])", tag)).unwrap();
    
    if let Some(caps) = re.captures(&content) {
        let inside = caps.get(2).unwrap().as_str();
        if inside.contains(target) {
            let mut parts: Vec<&str> = inside.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            parts.retain(|&x| x != target);
            
            let new_inside = parts.join(", ");
            let new_tag = format!("{}{}{}", caps.get(1).unwrap().as_str(), new_inside, caps.get(3).unwrap().as_str());
            content = content.replace(caps.get(0).unwrap().as_str(), &new_tag);
            fs::write(path, content)?;
        }
    }
    Ok(())
}
