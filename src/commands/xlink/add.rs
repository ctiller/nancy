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
use std::fs;
use std::path::PathBuf;

fn append_tag(path: &PathBuf, tag: &str, target: &PathBuf, is_rust: bool) -> Result<()> {
    let mut content = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let target_str = target.to_string_lossy().to_string();

    // rudimentary regex check if tag exists
    let re = regex::Regex::new(&format!(r"({}:\s*\[)(.*?)(\])", tag)).unwrap();
    
    if let Some(caps) = re.captures(&content) {
        let inside = caps.get(2).unwrap().as_str();
        if !inside.contains(&target_str) {
            let mut parts: Vec<&str> = inside.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
            if parts.contains(&"none") {
                parts.retain(|&x| x != "none");
            }
            parts.push(&target_str);
            let new_inside = parts.join(", ");
            let new_tag = format!("{}{}{}", caps.get(1).unwrap().as_str(), new_inside, caps.get(3).unwrap().as_str());
            content = content.replace(caps.get(0).unwrap().as_str(), &new_tag);
            fs::write(path, content)?;
        }
    } else {
        // Tag doesn't exist, append it.
        let new_line = if is_rust {
            format!("// {}: [{}]\n", tag, target_str)
        } else {
            format!("<!-- {}: [{}] -->\n", tag, target_str)
        };
        
        content.push_str("\n");
        content.push_str(&new_line);
        fs::write(path, content)?;
    }

    Ok(())
}

pub async fn run_add_implemented_by(cwd: PathBuf, doc: PathBuf, source: PathBuf) -> Result<()> {
    let doc_path = cwd.join(&doc);
    let source_path = cwd.join(&source);
    
    append_tag(&doc_path, "IMPLEMENTED_BY", &source, false)?;
    
    append_tag(&source_path, "DOCUMENTED_BY", &doc, source.extension().map_or(false, |e| e == "rs"))?;

    println!("Successfully linked {} <> {}", doc.display(), source.display());
    Ok(())
}

pub async fn run_add_documented_by(cwd: PathBuf, source: PathBuf, doc: PathBuf) -> Result<()> {
    run_add_implemented_by(cwd, doc, source).await
}

// IMPLEMENTED_BY: [source]

// DOCUMENTED_BY: [doc, docs/adr/0076-xlink-microformat.md]
