#![cfg(feature = "ssr")]

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


use git2::{IndexAddOption, Repository, Signature};
use sealed_test::prelude::*;
use std::fs;
use web::repo::{get_git_branches, get_repo_tree_ssr, read_file_text_ssr};

fn setup_repo() {
    let repo = Repository::init(".").expect("Failed to initialize repo");
    fs::write("test.txt", "hello world").unwrap();

    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();

    let sig = Signature::now("Test", "test@test.com").unwrap();
    let commit_id = repo
        .commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap();
    let commit = repo.find_commit(commit_id).unwrap();

    repo.branch("feature-branch", &commit, false).unwrap();

    // Modify on feature branch
    repo.set_head("refs/heads/feature-branch").unwrap();
    fs::create_dir("src_test").unwrap();
    fs::write("src_test/app.rs", "fn main() {}").unwrap();

    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Add app.rs", &tree, &[&parent])
        .unwrap();
}

#[sealed_test]
fn test_get_repo_tree_resolves_branches_statelessly() {
    setup_repo();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let master_nodes = get_repo_tree_ssr("master".to_string(), None).await.unwrap();
        assert_eq!(master_nodes.len(), 1);
        assert_eq!(master_nodes[0].name, "test.txt");

        let feat_nodes = get_repo_tree_ssr("feature-branch".to_string(), None)
            .await
            .unwrap();
        assert_eq!(feat_nodes.len(), 2);
        assert!(feat_nodes.iter().any(|n| n.name == "src_test"));
    });
}

#[sealed_test]
fn test_get_repo_tree_handles_subdirectories() {
    setup_repo();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let sub_nodes =
            get_repo_tree_ssr("feature-branch".to_string(), Some("src_test".to_string()))
                .await
                .unwrap();
        assert_eq!(sub_nodes.len(), 1);
        assert_eq!(sub_nodes[0].name, "app.rs");
    });
}

#[sealed_test]
fn test_read_file_text_extracts_syntax_blob() {
    setup_repo();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let text = read_file_text_ssr("master".to_string(), "test.txt".to_string())
            .await
            .unwrap();
        assert!(text.contains("hello world"));

        let app_text =
            read_file_text_ssr("feature-branch".to_string(), "src_test/app.rs".to_string())
                .await
                .unwrap();
        assert!(app_text.contains("fn"));
        assert!(app_text.contains("main"));

        let missing = read_file_text_ssr(
            "feature-branch".to_string(),
            "does_not_exist.rs".to_string(),
        )
        .await;
        assert!(missing.is_err());
    });
}

#[sealed_test]
fn test_get_git_branches_system_shell() {
    setup_repo();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let branches = get_git_branches().await.unwrap();
        assert!(branches.all_branches.contains(&"master".to_string()));
        assert!(
            branches
                .all_branches
                .contains(&"feature-branch".to_string())
        );
    });
}

// DOCUMENTED_BY: [docs/adr/README.md]
