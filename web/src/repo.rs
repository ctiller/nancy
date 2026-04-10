use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[cfg(feature = "ssr")]
pub async fn get_repo_tree_ssr(branch: String, dir: Option<String>) -> Result<Vec<FileNode>, ServerFnError> {
    use std::path::PathBuf;
    use git2::Repository;

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo = match Repository::discover(&root) {
        Ok(r) => r,
        Err(e) => return Err(ServerFnError::ServerError(format!("Git discovery failed: {}", e))),
    };

    // Fallback to active commit if branch is empty or strictly working directory
    let obj = match repo.revparse_single(&branch).or_else(|_| repo.revparse_single("HEAD")) {
        Ok(o) => o,
        Err(e) => return Err(ServerFnError::ServerError(format!("Revision not found: {}", e))),
    };
        
    let commit = match obj.peel_to_commit() {
        Ok(c) => c,
        Err(e) => return Err(ServerFnError::ServerError(format!("Not a commit: {}", e))),
    };
        
    let head_tree = match commit.tree() {
        Ok(t) => t,
        Err(e) => return Err(ServerFnError::ServerError(format!("Failed to get tree: {}", e))),
    };

    let mut nodes = Vec::new();
    
    // Resolve nested tree if directory is specified
    let target_tree = match &dir {
        Some(d) if !d.is_empty() => {
            let entry = match head_tree.get_path(std::path::Path::new(d)) {
                Ok(e) => e,
                Err(e) => return Err(ServerFnError::ServerError(format!("Path not found in tree: {}", e))),
            };
            let obj = match entry.to_object(&repo) {
                Ok(o) => o,
                Err(_) => return Err(ServerFnError::ServerError("Failed to resolve tree object".into())),
            };
            match obj.into_tree() {
                Ok(t) => t,
                Err(_) => return Err(ServerFnError::ServerError("Path is not a directory".into())),
            }
        },
        _ => head_tree
    };

    for entry in target_tree.iter() {
        if let Some(name) = entry.name() {
            let is_dir = entry.kind() == Some(git2::ObjectType::Tree);
            let rel_path = match &dir {
                Some(d) if !d.is_empty() => format!("{}/{}", d, name),
                _ => name.to_string(),
            };
            
            nodes.push(FileNode {
                name: name.to_string(),
                path: rel_path,
                is_dir,
            });
        }
    }

    nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
    Ok(nodes)
}

#[server(GetRepoTree, "/api")]
pub async fn get_repo_tree(branch: String, dir: Option<String>) -> Result<Vec<FileNode>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        get_repo_tree_ssr(branch, dir).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unimplemented!()
    }
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitBranchContext {
    pub active_branch: String,
    pub all_branches: Vec<String>,
}

#[server(GetGitBranches, "/api")]
pub async fn get_git_branches() -> Result<GitBranchContext, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use std::process::Command;
        
        let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        // Get all branches
        let output = match Command::new("git")
            .arg("branch")
            .arg("--format=%(refname:short)")
            .current_dir(&root)
            .output() {
                Ok(o) => o,
                Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
            };
            
        let mut branches = Vec::new();
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let br = line.trim().to_string();
                if !br.is_empty() {
                    branches.push(br);
                }
            }
        }
        
        // Get active branch
        let active_output = match Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(&root)
            .output() {
                Ok(o) => o,
                Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
            };
            
        let mut active_branch = String::new();
        if active_output.status.success() {
            active_branch = String::from_utf8_lossy(&active_output.stdout).trim().to_string();
        }
        
        Ok(GitBranchContext {
            active_branch,
            all_branches: branches,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        unimplemented!()
    }
}



#[cfg(feature = "ssr")]
pub async fn read_file_text_ssr(branch: String, path: String) -> Result<String, ServerFnError> {
    use std::path::{Path, PathBuf};
    use git2::Repository;

    let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo = match Repository::discover(&root) {
        Ok(r) => r,
        Err(e) => return Err(ServerFnError::ServerError(format!("Git discovery failed: {}", e))),
    };

    let obj = match repo.revparse_single(&branch).or_else(|_| repo.revparse_single("HEAD")) {
        Ok(o) => o,
        Err(e) => return Err(ServerFnError::ServerError(format!("Revision not found: {}", e))),
    };
        
    let commit = match obj.peel_to_commit() {
        Ok(c) => c,
        Err(e) => return Err(ServerFnError::ServerError(format!("Not a commit: {}", e))),
    };
        
    let head_tree = match commit.tree() {
        Ok(t) => t,
        Err(e) => return Err(ServerFnError::ServerError(format!("Failed to get tree: {}", e))),
    };
        
    let entry = match head_tree.get_path(Path::new(&path)) {
        Ok(e) => e,
        Err(e) => return Err(ServerFnError::ServerError(format!("File not found in tree: {}", e))),
    };
        
    let blob = match entry.to_object(&repo) {
        Ok(o) => match o.into_blob() {
            Ok(b) => b,
            Err(_) => return Err(ServerFnError::ServerError("Path is not a file".into())),
        },
        Err(_) => return Err(ServerFnError::ServerError("Failed to resolve object".into())),
    };

    let content = match String::from_utf8(blob.content().to_vec()) {
        Ok(c) => c,
        Err(_) => return Err(ServerFnError::ServerError("File is not valid UTF-8 text".into())),
    };

    let ext = Path::new(&path).extension().and_then(|s| s.to_str()).unwrap_or("");

    // 3. Highlight
    if let Some(highlighted) = tree_sitter_highlight_file(ext, &content) {
        return Ok(highlighted);
    }

    Ok(syntect_highlight_file(ext, &content))
}

#[server(ReadFileText, "/api")]
pub async fn read_file_text(branch: String, path: String) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        read_file_text_ssr(branch, path).await
    }
    #[cfg(not(feature = "ssr"))]
    {
        unimplemented!()
    }
}


#[cfg(feature = "ssr")]
fn tree_sitter_highlight_file(ext: &str, content: &str) -> Option<String> {
    use tree_sitter_highlight::{HighlightConfiguration, Highlighter, HtmlRenderer};

    // Mapping configurations manually here
    let (language, name, highlight_query, injections_query, locals_query) = match ext {
        "rs" => (
            tree_sitter_rust::LANGUAGE.into(),
            "rust",
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        ),
        "md" => (
            tree_sitter_md::LANGUAGE.into(),
            "markdown",
            tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
            "",
            "",
        ),
        "ts" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            "typescript",
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
            "",
            tree_sitter_typescript::LOCALS_QUERY,
        ),
        "js" => (
            tree_sitter_javascript::LANGUAGE.into(),
            "javascript",
            tree_sitter_javascript::HIGHLIGHT_QUERY,
            tree_sitter_javascript::INJECTIONS_QUERY,
            tree_sitter_javascript::LOCALS_QUERY,
        ),
        _ => return None,
    };

    let mut config = HighlightConfiguration::new(
        language,
        name,
        highlight_query,
        injections_query,
        locals_query,
    ).ok()?;

    // Simple mapping for CSS classes matching syntect or our dark mode
    let highlight_names = [
        "attribute", "constant", "function.builtin", "function", "keyword",
        "operator", "property", "punctuation", "punctuation.bracket",
        "punctuation.delimiter", "string", "string.special", "tag", "type",
        "type.builtin", "variable", "variable.builtin", "variable.parameter",
    ];

    config.configure(&highlight_names);

    let mut highlighter = Highlighter::new();
    let highlights = highlighter
        .highlight(&config, content.as_bytes(), None, |_| None)
        .ok()?;

    let mut renderer = HtmlRenderer::new();
    renderer
        .render(highlights, content.as_bytes(), &|highlight, out| {
            out.extend_from_slice(format!("class=\"ts-{}\"", highlight_names[highlight.0].replace('.', "-")).as_bytes());
        })
        .ok()?;

    let html = String::from_utf8(renderer.html).ok()?;
    Some(format!("<pre class=\"tree-sitter-wrapper\"><code>{}</code></pre>", html))
}

#[cfg(feature = "ssr")]
fn syntect_highlight_file(ext: &str, content: &str) -> String {
    use syntect::easy::HighlightLines;
    use syntect::html::{styled_line_to_highlighted_html, IncludeBackground};
    use syntect::parsing::SyntaxSet;
    use syntect::highlighting::{ThemeSet, Style};

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    
    // We use base16-ocean.dark or similar dark theme to match glassmorphism
    let theme = &ts.themes["base16-ocean.dark"];

    let syntax = ps.find_syntax_by_extension(ext).unwrap_or_else(|| ps.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    let mut html = String::new();
    for line in content.lines() {
        // Appending newline since lines() strips them and syntect needs them internally 
        // to maintain proper blocking if we were using ranges, but for line-by-line it's okay.
        let line_with_nl = format!("{}\n", line);
        let ranges: Vec<(Style, &str)> = h.highlight_line(&line_with_nl, &ps).unwrap();
        let escaped = styled_line_to_highlighted_html(&ranges, IncludeBackground::No).unwrap();
        html.push_str(&escaped);
    }

    format!("<pre class=\"syntect-wrapper\"><code>{}</code></pre>", html)
}
