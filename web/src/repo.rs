use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

#[server(GetRepoTree, "/api")]
pub async fn get_repo_tree(dir: Option<String>) -> Result<Vec<FileNode>, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ignore::WalkBuilder;
        use std::path::PathBuf;

        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let target_dir = match dir {
            Some(d) => root.join(d),
            None => root.clone(),
        };

        // Enforce boundary strictly within the root git repo for security
        if !target_dir.starts_with(&root) {
            return Err(ServerFnError::ServerError("Security violation: path out of bounds".into()));
        }

        let mut nodes = Vec::new();

        let walker = WalkBuilder::new(&target_dir).max_depth(Some(1)).build();

        for result in walker {
            match result {
                Ok(entry) => {
                    let path = entry.path();
                    if path == target_dir {
                        continue; // Skip the directory itself
                    }

                    let rel_path = path
                        .strip_prefix(&root)
                        .unwrap_or(path)
                        .to_string_lossy()
                        .to_string();
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

                    nodes.push(FileNode {
                        name,
                        path: rel_path,
                        is_dir,
                    });
                }
                Err(_) => continue,
            }
        }

        // Sort: Directories first, then alphabetical
        nodes.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));

        Ok(nodes)
    }
    #[cfg(not(feature = "ssr"))]
    {
        unimplemented!()
    }
}

#[server(ReadFileText, "/api")]
pub async fn read_file_text(path: String) -> Result<String, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        use ignore::WalkBuilder;
        use std::fs;
        use std::path::{Path, PathBuf};

        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let target = root.join(&path);

        if !target.starts_with(&root) {
            return Err(ServerFnError::ServerError("Security violation".into()));
        }

        // 1. Verify Gitignore Rules exist for file
        let mut is_ignored = true;
        for result in WalkBuilder::new(&target).max_depth(Some(0)).build() {
            if result.is_ok() {
                is_ignored = false;
                break;
            }
        }

        if is_ignored {
            return Err(ServerFnError::ServerError("File is computationally ignored or omitted.".into()));
        }

        // 2. Read Text
        let content = match fs::read_to_string(&target) {
            Ok(c) => c,
            Err(e) => return Err(e.into()),
        };

        let ext = Path::new(&path).extension().and_then(|s| s.to_str()).unwrap_or("");

        // 3. Highlight
        if let Some(highlighted) = tree_sitter_highlight_file(ext, &content) {
            return Ok(highlighted);
        }

        Ok(syntect_highlight_file(ext, &content))
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
