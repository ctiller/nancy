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

use git2::BranchType;
use std::path::PathBuf;
use tokio::sync::oneshot;

use super::types::{BranchProxy, CommitProxy, OidProxy, ReferenceProxy};

pub enum GitRequest {
    Init {
        path: PathBuf,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    Discover {
        path: PathBuf,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    Open {
        path: PathBuf,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    FindReference {
        name: String,
        resp: oneshot::Sender<anyhow::Result<ReferenceProxy>>,
    },
    PeelToCommit {
        reference: String,
        resp: oneshot::Sender<anyhow::Result<CommitProxy>>,
    },
    FindObject {
        oid: String,
        resp: oneshot::Sender<anyhow::Result<Vec<u8>>>,
    },
    Branch {
        name: String,
        commit_oid: String,
        force: bool,
        resp: oneshot::Sender<anyhow::Result<BranchProxy>>,
    },
    Branches {
        filter: Option<BranchType>,
        resp: oneshot::Sender<anyhow::Result<Vec<BranchProxy>>>,
    },
    Add {
        pathspecs: Vec<String>,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    CommitTree {
        message: String,
        author_name: String,
        author_email: String,
        tree_oid: Option<String>,
        parents: Vec<String>,
        resp: oneshot::Sender<anyhow::Result<OidProxy>>,
    },
    Checkout {
        branch: String,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    Merge {
        branch_name: String,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    Push {
        remote: String,
        refspecs: Vec<String>,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    Fetch {
        remote: String,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
    GetFeatureBranch {
        task_id: String,
        resp: oneshot::Sender<anyhow::Result<Option<String>>>,
    },
    Log {
        reference: String,
        max_count: usize,
        resp: oneshot::Sender<anyhow::Result<Vec<CommitProxy>>>,
    },
    DiffTreeToTree {
        begin_oid: String,
        end_oid: String,
        resp: oneshot::Sender<anyhow::Result<String>>,
    },
    RunProcess {
        args: Vec<String>,
        dir: Option<PathBuf>,
        resp: oneshot::Sender<anyhow::Result<String>>,
    },
    RevparseSingle {
        spec: String,
        resp: oneshot::Sender<anyhow::Result<OidProxy>>,
    },
    ReadTree {
        oid: String,
        resp: oneshot::Sender<anyhow::Result<Vec<(String, String, Option<git2::ObjectType>)>>>,
    },
    ReadBlob {
        oid: String,
        resp: oneshot::Sender<anyhow::Result<Vec<u8>>>,
    },
    CommitBlobBatch {
        refname: String,
        events_blobs: Vec<(String, Vec<u8>)>,
        incidents_blobs: Vec<(String, Vec<u8>)>,
        resp: oneshot::Sender<anyhow::Result<bool>>,
    },
    SetIntrospection {
        ctx: crate::introspection::IntrospectionContext,
        resp: oneshot::Sender<anyhow::Result<()>>,
    },
}
