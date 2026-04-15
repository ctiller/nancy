use anyhow::Context;
use git2::BranchType;
use std::path::Path;
use tokio::sync::{mpsc, oneshot};

use super::messages::GitRequest;
use super::types::{BranchProxy, CommitProxy, OidProxy, ReferenceProxy};

#[derive(Clone)]
pub struct AsyncRepository {
    tx: mpsc::Sender<GitRequest>,
    workdir: std::path::PathBuf,
}

impl AsyncRepository {
    async fn send_req(&self, req: GitRequest) -> anyhow::Result<()> {
        self.tx.send(req).await.map_err(|e| anyhow::anyhow!("MPSC send error: {}", e))
    }
    
    pub async fn attach_introspection(&self, ctx: crate::introspection::IntrospectionContext) {
        let (resp, _rx) = oneshot::channel();
        let _ = self.send_req(GitRequest::SetIntrospection { ctx, resp }).await;
    }
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            tx: super::actor::GitActor::spawn(),
            workdir: path.as_ref().to_path_buf(),
        }
    }

    pub fn workdir(&self) -> Option<std::path::PathBuf> {
        Some(self.workdir.clone())
    }

    pub fn path(&self) -> std::path::PathBuf {
        self.workdir.clone()
    }

    pub async fn init(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let instance = Self::new(path.as_ref());
        let (resp_tx, resp_rx) = oneshot::channel();
        instance.send_req(GitRequest::Init {
                path: path.as_ref().to_path_buf(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")??;
        Ok(instance)
    }

    pub async fn discover(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let instance = Self::new(path.as_ref());
        let (resp_tx, resp_rx) = oneshot::channel();
        instance.send_req(GitRequest::Discover {
                path: path.as_ref().to_path_buf(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")??;
        Ok(instance)
    }

    pub async fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let instance = Self::new(path.as_ref());
        let (resp_tx, resp_rx) = oneshot::channel();
        instance.send_req(GitRequest::Open {
                path: path.as_ref().to_path_buf(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")??;
        Ok(instance)
    }

    pub async fn find_reference(&self, name: &str) -> anyhow::Result<ReferenceProxy> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::FindReference {
                name: name.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn peel_to_commit(&self, reference: &str) -> anyhow::Result<CommitProxy> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::PeelToCommit {
                reference: reference.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn find_object(&self, oid: &str) -> anyhow::Result<Vec<u8>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::FindObject {
                oid: oid.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn branch(
        &self,
        name: &str,
        commit_oid: &str,
        force: bool,
    ) -> anyhow::Result<BranchProxy> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Branch {
                name: name.to_string(),
                commit_oid: commit_oid.to_string(),
                force,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn branches(&self, filter: Option<BranchType>) -> anyhow::Result<Vec<BranchProxy>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Branches {
                filter,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn add(&self, pathspecs: Vec<String>) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Add {
                pathspecs,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn commit_tree(
        &self,
        message: &str,
        author_name: &str,
        author_email: &str,
        tree_oid: Option<String>,
        parents: Vec<String>,
    ) -> anyhow::Result<OidProxy> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::CommitTree {
                message: message.to_string(),
                author_name: author_name.to_string(),
                author_email: author_email.to_string(),
                tree_oid,
                parents,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn checkout(&self, branch: &str) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Checkout {
                branch: branch.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn merge(&self, branch_name: &str) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Merge {
                branch_name: branch_name.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn push(&self, remote: &str, refspecs: Vec<String>) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Push {
                remote: remote.to_string(),
                refspecs,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn fetch(&self, remote: &str) -> anyhow::Result<()> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Fetch {
                remote: remote.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn get_feature_branch(&self, task_id: &str) -> anyhow::Result<Option<String>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::GetFeatureBranch {
                task_id: task_id.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn log(&self, reference: &str, max_count: usize) -> anyhow::Result<Vec<CommitProxy>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::Log {
                reference: reference.to_string(),
                max_count,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn diff_tree_to_tree(
        &self,
        begin_oid: &str,
        end_oid: &str,
    ) -> anyhow::Result<String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::DiffTreeToTree {
                begin_oid: begin_oid.to_string(),
                end_oid: end_oid.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn run_process(
        &self,
        args: Vec<String>,
        dir: Option<std::path::PathBuf>,
    ) -> anyhow::Result<String> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::RunProcess {
                args,
                dir,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn read_tree(
        &self,
        oid: &str,
    ) -> anyhow::Result<Vec<(String, String, Option<git2::ObjectType>)>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::ReadTree {
                oid: oid.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn read_blob(&self, oid: &str) -> anyhow::Result<Vec<u8>> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::ReadBlob {
                oid: oid.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn revparse_single(&self, spec: &str) -> anyhow::Result<OidProxy> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::RevparseSingle {
                spec: spec.to_string(),
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }

    pub async fn commit_blob_batch(
        &self,
        refname: &str,
        events_blobs: Vec<(String, Vec<u8>)>,
        incidents_blobs: Vec<(String, Vec<u8>)>,
    ) -> anyhow::Result<bool> {
        let (resp_tx, resp_rx) = oneshot::channel();
        self.send_req(GitRequest::CommitBlobBatch {
                refname: refname.to_string(),
                events_blobs,
                incidents_blobs,
                resp: resp_tx,
            })
            .await?;
        resp_rx.await.context("Actor thread closed")?
    }
}

// DOCUMENTED_BY: [docs/adr/0065-async-git-actor.md]
