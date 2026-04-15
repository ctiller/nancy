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

use anyhow::{Context, anyhow};
use git2::{BranchType, ObjectType, Oid, Repository, Signature};
use std::thread;
use tokio::sync::mpsc;

use super::messages::GitRequest;
use super::types::{BranchProxy, CommitProxy, OidProxy, ReferenceProxy};

pub struct GitActor {
    repo: Option<Repository>,
    receiver: mpsc::Receiver<GitRequest>,
    ctx: Option<crate::introspection::IntrospectionContext>,
}

impl GitActor {
    pub fn spawn() -> mpsc::Sender<GitRequest> {
        let (tx, rx) = mpsc::channel(32);
        thread::spawn(move || {
            let mut actor = GitActor {
                repo: None,
                receiver: rx,
                ctx: None,
            };
            actor.run();
        });
        tx
    }

    fn run(&mut self) {
        while let Some(msg) = self.receiver.blocking_recv() {
            let req_name = match &msg {
                GitRequest::Init { .. } => "GitRequest::Init",
                GitRequest::Discover { .. } => "GitRequest::Discover",
                GitRequest::Open { .. } => "GitRequest::Open",
                GitRequest::FindReference { .. } => "GitRequest::FindReference",
                GitRequest::PeelToCommit { .. } => "GitRequest::PeelToCommit",
                GitRequest::FindObject { .. } => "GitRequest::FindObject",
                GitRequest::Branch { .. } => "GitRequest::Branch",
                GitRequest::Branches { .. } => "GitRequest::Branches",
                GitRequest::Add { .. } => "GitRequest::Add",
                GitRequest::CommitTree { .. } => "GitRequest::CommitTree",
                GitRequest::Checkout { .. } => "GitRequest::Checkout",
                GitRequest::Merge { .. } => "GitRequest::Merge",
                GitRequest::Push { .. } => "GitRequest::Push",
                GitRequest::Fetch { .. } => "GitRequest::Fetch",
                GitRequest::GetFeatureBranch { .. } => "GitRequest::GetFeatureBranch",
                GitRequest::Log { .. } => "GitRequest::Log",
                GitRequest::DiffTreeToTree { .. } => "GitRequest::DiffTreeToTree",
                GitRequest::RunProcess { .. } => "GitRequest::RunProcess",
                GitRequest::RevparseSingle { .. } => "GitRequest::RevparseSingle",
                GitRequest::ReadTree { .. } => "GitRequest::ReadTree",
                GitRequest::ReadBlob { .. } => "GitRequest::ReadBlob",
                GitRequest::CommitBlobBatch { .. } => "GitRequest::CommitBlobBatch",
                GitRequest::SetIntrospection { .. } => "GitRequest::SetIntrospection",
            };

            // Don't recurse introspection setup inside itself
            if req_name == "GitRequest::SetIntrospection" {
                self.handle_msg(msg);
                continue;
            }

            if let Some(ctx) = self.ctx.clone() {
                let _ = ctx.in_frame(req_name, |_child_ctx| {
                    self.handle_msg(msg)
                });
            } else {
                self.handle_msg(msg);
            }
        }
    }

    fn handle_msg(&mut self, msg: GitRequest) {
        match msg {
            GitRequest::Init { path, resp } => {
                    let res = Repository::init(&path)
                        .map(|r| {
                            self.repo = Some(r);
                        })
                        .map_err(Into::into);
                    let _ = resp.send(res);
                }
                GitRequest::Discover { path, resp } => {
                    let res = Repository::discover(&path)
                        .map(|r| {
                            self.repo = Some(r);
                        })
                        .map_err(Into::into);
                    let _ = resp.send(res);
                }
                GitRequest::Open { path, resp } => {
                    let res = Repository::open(&path)
                        .map(|r| {
                            self.repo = Some(r);
                        })
                        .map_err(Into::into);
                    let _ = resp.send(res);
                }
                GitRequest::FindReference { name, resp } => {
                    let res = self.with_repo(|repo| {
                        let reference = repo.find_reference(&name)?;
                        Ok(ReferenceProxy::new(&reference))
                    });
                    let _ = resp.send(res);
                }
                GitRequest::PeelToCommit { reference, resp } => {
                    let res = self.with_repo(|repo| {
                        let r = repo.find_reference(&reference)?;
                        let commit = r.peel_to_commit()?;
                        Ok(CommitProxy::new(&commit))
                    });
                    let _ = resp.send(res);
                }
                GitRequest::FindObject { oid, resp } => {
                    let res = self.with_repo(|repo| {
                        let id = Oid::from_str(&oid)?;
                        let obj = repo.find_object(id, None)?;
                        let mut buf = Vec::new();
                        // This might be tricky. The user requested `FindObject` returning `Vec<u8>`.
                        // If it's a blob:
                        if let Some(blob) = obj.as_blob() {
                            buf.extend_from_slice(blob.content());
                        }
                        Ok(buf)
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Branch {
                    name,
                    commit_oid,
                    force,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let id = Oid::from_str(&commit_oid)?;
                        let commit = repo.find_commit(id)?;
                        let b = repo.branch(&name, &commit, force)?;
                        Ok(BranchProxy::new(&b, BranchType::Local))
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Branches { filter, resp } => {
                    let res = self.with_repo(|repo| {
                        let mut branches = Vec::new();
                        let iter = repo.branches(filter)?;
                        for result in iter {
                            let (branch, branch_type) = result?;
                            branches.push(BranchProxy::new(&branch, branch_type));
                        }
                        Ok(branches)
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Add { pathspecs, resp } => {
                    let res = self.with_repo(|repo| {
                        let mut index = repo.index()?;
                        index.add_all(pathspecs, git2::IndexAddOption::DEFAULT, None)?;
                        index.write()?;
                        Ok(())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::CommitTree {
                    message,
                    author_name,
                    author_email,
                    tree_oid,
                    parents,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let sig = Signature::now(&author_name, &author_email)?;

                        let tree = if let Some(t_id) = tree_oid {
                            let oid = Oid::from_str(&t_id)?;
                            repo.find_tree(oid)?
                        } else {
                            let mut index = repo.index()?;
                            let oid = index.write_tree()?;
                            repo.find_tree(oid)?
                        };

                        let mut parent_commits = Vec::new();
                        for p_str in parents {
                            let id = Oid::from_str(&p_str)?;
                            parent_commits.push(repo.find_commit(id)?);
                        }

                        let parent_refs: Vec<&git2::Commit> = parent_commits.iter().collect();

                        // we will update HEAD by default here if passing "HEAD"
                        let oid =
                            repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parent_refs)?;
                        Ok(OidProxy::new(oid))
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Checkout { branch, resp } => {
                    let res = self.with_repo(|repo| {
                        let b = repo.find_branch(&branch, BranchType::Local)?;
                        let obj = b.get().peel(ObjectType::Any)?;
                        repo.checkout_tree(&obj, None)?;
                        repo.set_head(b.get().name().unwrap())?;
                        Ok(())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Merge { branch_name, resp } => {
                    let res = self.with_repo(|repo| {
                        let fetch_head = repo.find_reference(&branch_name)
                            .or_else(|_| repo.find_reference(&format!("refs/heads/{}", branch_name)))?;
                        let fetch_commit = repo.reference_to_annotated_commit(&fetch_head)?;

                        // Merge analysis
                        let (analysis, _) = repo.merge_analysis(&[&fetch_commit])?;
                        if analysis.is_up_to_date() {
                            return Ok(());
                        } else if analysis.is_fast_forward() {
                            let refname =
                                format!("refs/heads/{}", repo.head()?.shorthand().unwrap());
                            let mut reference = repo.find_reference(&refname)?;
                            reference.set_target(fetch_commit.id(), "Fast-Forward")?;
                            repo.set_head(&refname)?;
                            repo.checkout_head(Some(
                                git2::build::CheckoutBuilder::default().force(),
                            ))?;
                        } else {
                            // full merge
                            return Err(anyhow!(
                                "Full merges not automatically supported via single Merge call"
                            ));
                        }
                        Ok(())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Push {
                    remote,
                    refspecs,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let mut r = repo.find_remote(&remote)?;
                        r.push(&refspecs, None)?;
                        Ok(())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Fetch { remote, resp } => {
                    let res = self.with_repo(|repo| {
                        let mut r = repo.find_remote(&remote)?;
                        r.fetch(&[] as &[&str], None, None)?;
                        Ok(())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::GetFeatureBranch { task_id, resp } => {
                    let res = self.with_repo(|repo| {
                        // find branch nancy/tasks/{task_id}
                        let name = format!("nancy/tasks/{}", task_id);
                        if let Ok(_) = repo.find_branch(&name, BranchType::Local) {
                            Ok(Some(name))
                        } else {
                            Ok(None)
                        }
                    });
                    let _ = resp.send(res);
                }
                GitRequest::Log {
                    reference,
                    max_count,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let mut revwalk = repo.revwalk()?;
                        let r = repo.find_reference(&reference)?;
                        let commit_oid = r.peel_to_commit()?.id();
                        revwalk.push(commit_oid)?;

                        let mut results = Vec::new();
                        for oid_res in revwalk.take(max_count) {
                            let id = oid_res?;
                            let commit = repo.find_commit(id)?;
                            results.push(CommitProxy::new(&commit));
                        }
                        Ok(results)
                    });
                    let _ = resp.send(res);
                }
                GitRequest::DiffTreeToTree {
                    begin_oid,
                    end_oid,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let id1 = Oid::from_str(&begin_oid)?;
                        let id2 = Oid::from_str(&end_oid)?;

                        let t1 = match repo.find_commit(id1) {
                            Ok(c) => c.tree()?,
                            Err(_) => repo.find_tree(id1)?,
                        };

                        let t2 = match repo.find_commit(id2) {
                            Ok(c) => c.tree()?,
                            Err(_) => repo.find_tree(id2)?,
                        };

                        let diff = repo.diff_tree_to_tree(Some(&t1), Some(&t2), None)?;
                        let mut diff_text = String::new();
                        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                            let origin = line.origin();
                            if origin == '+' || origin == '-' || origin == ' ' {
                                diff_text.push(origin);
                            }
                            diff_text.push_str(std::str::from_utf8(line.content()).unwrap_or(""));
                            true
                        })?;
                        Ok(diff_text)
                    });
                    let _ = resp.send(res);
                }
                GitRequest::RunProcess { args, dir, resp } => {
                    let mut cmd = std::process::Command::new("git");
                    cmd.args(&args);

                    if let Some(d) = dir {
                        cmd.current_dir(&d);
                    } else if let Some(r) = &self.repo {
                        if let Some(w) = r.workdir() {
                            cmd.current_dir(w);
                        }
                    }

                    let res = match cmd.output() {
                        Ok(output) => {
                            let mut s = String::from_utf8_lossy(&output.stdout).to_string();
                            if !output.status.success() {
                                s.push_str("\nSTDERR:\n");
                                s.push_str(&String::from_utf8_lossy(&output.stderr));
                                Err(anyhow::anyhow!("Process failed: {}", s))
                            } else {
                                Ok(s)
                            }
                        }
                        Err(e) => Err(anyhow::anyhow!("Process execution error: {}", e)),
                    };
                    let _ = resp.send(res);
                }
                GitRequest::RevparseSingle { spec, resp } => {
                    let res = self.with_repo(|repo| {
                        let obj = repo.revparse_single(&spec)?;
                        Ok(OidProxy::new(obj.id()))
                    });
                    let _ = resp.send(res);
                }
                GitRequest::ReadTree { oid, resp } => {
                    let res = self.with_repo(|repo| {
                        let id = Oid::from_str(&oid)?;
                        let tree = repo.find_tree(id)?;
                        let mut entries = Vec::new();
                        for entry in tree.iter() {
                            if let Some(name) = entry.name() {
                                entries.push((
                                    name.to_string(),
                                    entry.id().to_string(),
                                    entry.kind(),
                                ));
                            }
                        }
                        Ok(entries)
                    });
                    let _ = resp.send(res);
                }
                GitRequest::ReadBlob { oid, resp } => {
                    let res = self.with_repo(|repo| {
                        let id = Oid::from_str(&oid)?;
                        let blob = repo.find_blob(id)?;
                        Ok(blob.content().to_vec())
                    });
                    let _ = resp.send(res);
                }
                GitRequest::CommitBlobBatch {
                    refname,
                    events_blobs,
                    incidents_blobs,
                    resp,
                } => {
                    let res = self.with_repo(|repo| {
                        let branch_ref = repo.find_reference(&refname);
                        let branch_commit = if let Ok(br) = &branch_ref {
                            br.peel_to_commit().ok()
                        } else {
                            None
                        };

                        let events_tree = if let Some(commit) = &branch_commit {
                            let tree = commit.tree()?;
                            if let Ok(entry) = tree.get_name("events").context("events miss") {
                                entry.to_object(repo)?.into_tree().ok()
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let incidents_tree = if let Some(commit) = &branch_commit {
                            let tree = commit.tree()?;
                            if let Some(entry) = tree.get_name("incidents") {
                                entry.to_object(repo)?.into_tree().ok()
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let mut events_tb = repo.treebuilder(events_tree.as_ref())?;
                        for (name, content) in events_blobs {
                            let blob_id = repo.blob(&content)?;
                            events_tb.insert(name, blob_id, 0o100644)?;
                        }
                        let new_events_tree_id = events_tb.write()?;

                        let mut incidents_tb = repo.treebuilder(incidents_tree.as_ref())?;
                        for (name, content) in incidents_blobs {
                            let blob_id = repo.blob(&content)?;
                            incidents_tb.insert(name, blob_id, 0o100644)?;
                        }
                        let new_incidents_tree_id = incidents_tb.write()?;

                        let mut parents = Vec::new();
                        let root_tree_id = if let Some(commit) = &branch_commit {
                            parents.push(commit);
                            let mut root_tb = repo.treebuilder(Some(&commit.tree()?))?;
                            root_tb.insert("events", new_events_tree_id, 0o040000)?;
                            root_tb.insert("incidents", new_incidents_tree_id, 0o040000)?;
                            root_tb.write()?
                        } else {
                            let mut root_tb = repo.treebuilder(None)?;
                            root_tb.insert("events", new_events_tree_id, 0o040000)?;
                            root_tb.insert("incidents", new_incidents_tree_id, 0o040000)?;
                            root_tb.write()?
                        };

                        let new_root_tree = repo.find_tree(root_tree_id)?;
                        let parents_refs: Vec<&git2::Commit> = parents.into_iter().collect();

                        let sig = repo.signature().unwrap_or_else(|_| {
                            git2::Signature::now("Nancy Orchestrator", "nancy@localhost").unwrap()
                        });

                        repo.commit(
                            Some(&refname),
                            &sig,
                            &sig,
                            "Batched append event logs",
                            &new_root_tree,
                            &parents_refs,
                        )?;

                        Ok(true)
                    });
                    let _ = resp.send(res);
                }
            GitRequest::SetIntrospection { ctx, resp } => {
                self.ctx = Some(ctx);
                let _ = resp.send(Ok(()));
            }
        }
    }

    fn with_repo<T, F>(&mut self, f: F) -> anyhow::Result<T>
    where
        F: FnOnce(&Repository) -> anyhow::Result<T>,
    {
        if let Some(repo) = &self.repo {
            f(repo)
        } else {
            Err(anyhow!("Repository not initialized or discovered yet"))
        }
    }
}
