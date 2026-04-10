use anyhow::{Context, Result};
use bollard::Docker;
use bollard::models::{ContainerCreateBody as Config, HostConfig};
use bollard::query_parameters::{
    CreateContainerOptions, CreateImageOptions, StartContainerOptions,
};
use futures_util::stream::StreamExt;
use std::collections::{HashMap, HashSet};
use rand::Rng;
use tokio::fs;
use std::path::{Path, PathBuf};

use crate::coordinator::appview::AppView;
use crate::schema::identity_config::Identity;

fn build_worker_env_vars(coordinator_did: &str) -> Vec<String> {
    let mut env_vars = vec![format!("COORDINATOR_DID={}", coordinator_did)];
    if let Ok(api_key) = std::env::var("GEMINI_API_KEY") {
        env_vars.push(format!("GEMINI_API_KEY={}", api_key));
    }
    if let Ok(rust_log) = std::env::var("RUST_LOG") {
        env_vars.push(format!("RUST_LOG={}", rust_log));
    }
    env_vars
}

pub async fn build_container_config(
    rt_image: &str,
    target_path: &Path,
    host_workdir: &Path,
    worker_did: &str,
    env_vars: Vec<String>,
) -> Config {
    let canonical_path = target_path
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| target_path.display().to_string());
        
    let host_workdir_str = host_workdir
        .canonicalize()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| host_workdir.display().to_string());

    // To support git worktree absolute path references in sandboxes securely,
    // we must mount the host workdir natively. To enforce strict sandboxing, 
    // we mount the root repo read-only (so agents cannot touch other host files),
    // and we predictably override `.git` and the specific worktree as read-write!
    let mut binds = vec![
        format!("{}:{}:ro", host_workdir_str, host_workdir_str),
        format!("{}/.git:{}/.git:rw", host_workdir_str, host_workdir_str),
        format!("{}:{}:rw", canonical_path, canonical_path),
    ];

    let mut env_vars = env_vars;
    env_vars.push(format!("NANCY_GRINDER_SOCKET_PATH=/tmp/nancy_sockets/{}/grinder.sock", worker_did));
    env_vars.push("NANCY_COORDINATOR_SOCKET_PATH=/tmp/nancy_sockets/coordinator/coordinator.sock".to_string());
    env_vars.push("SSL_CERT_DIR=/etc/ssl/certs".to_string());
    env_vars.push("SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt".to_string());

    // Secure isolated socket tunneling mapped into `/tmp` resolving arbitrary linux SUN_LEN directory limits gracefully
    binds.push(format!(
        "{}/.nancy/sockets/{}:/tmp/nancy_sockets/{}:rw",
        host_workdir_str, worker_did, worker_did
    ));
    
    // Shared isolated socket tunnel for coordinator UDS (ro) bounded via explicit short path
    binds.push(format!(
        "{}/.nancy/sockets/coordinator:/tmp/nancy_sockets/coordinator:ro",
        host_workdir_str
    ));

    let host_config = HostConfig {
        binds: Some(binds),
        ..Default::default()
    };

    let uid = String::from_utf8(tokio::process::Command::new("id").arg("-u").output().await.unwrap().stdout).unwrap().trim().to_string();
    let gid = String::from_utf8(tokio::process::Command::new("id").arg("-g").output().await.unwrap().stdout).unwrap().trim().to_string();

    Config {
        image: Some(rt_image.to_string()),
        user: Some(format!("{}:{}", uid, gid)),
        cmd: Some(vec!["/nancy".to_string(), "grind".to_string()]),
        env: Some(env_vars),
        host_config: Some(host_config),
        working_dir: Some(canonical_path),
        ..Default::default()
    }
}

pub struct ContainerState {
    pub name: String,
    pub failures: u32,
    pub next_restart_allowed_at: Option<std::time::Instant>,
}

pub struct DockerOrchestrator {
    docker: Docker,
    workdir: PathBuf,
    active_containers: HashSet<String>,
    crash_backoffs: HashMap<String, ContainerState>,
}

impl DockerOrchestrator {
    pub fn new(workdir: PathBuf) -> Result<Self> {
        let docker = Docker::connect_with_local_defaults().context("Failed to connect to local Docker")?;
        Ok(Self {
            docker,
            workdir,
            active_containers: HashSet::new(),
            crash_backoffs: HashMap::new(),
        })
    }

    pub async fn sync_deployments(&mut self, _appview: &AppView, identity: &Identity) -> Vec<(crate::schema::task::AgentCrashReportPayload, String)> {
        let root_did = identity.get_did_owner().did.clone();

        let workers = match identity {
            Identity::Coordinator { workers, .. } => workers.clone(),
            _ => return Vec::new(), // Grinders don't launch docker containers
        };

        if workers.is_empty() {
            return Vec::new();
        }

        // Make sure base image is ready
        let rt_image = "rust:latest";
        let mut pull_stream = self.docker.create_image(
            Some(CreateImageOptions {
                from_image: Some(rt_image.to_string()),
                ..Default::default()
            }),
            None,
            None,
        );
        while let Some(res) = pull_stream.next().await {
            if let Err(e) = res {
                tracing::warn!("Docker image pull partial error: {}", e);
            }
        }

        let exe_path = std::env::var("NANCY_E2E_EXECUTABLE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_exe().unwrap());
            
        let mut cached_tar_payload = None;
        if let Ok(exe_data) = fs::read(&exe_path).await {
            let mut tar_builder = tar::Builder::new(Vec::new());
            let mut header = tar::Header::new_gnu();
            header.set_path("nancy").unwrap();
            header.set_size(exe_data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            if tar_builder.append(&header, exe_data.as_slice()).is_ok() {
                if let Ok(payload) = tar_builder.into_inner() {
                    cached_tar_payload = Some(payload);
                }
            }
        }

        let mut crash_reports = Vec::new();
        let mut to_remove = Vec::new();
        for container_name in &self.active_containers {
            if let Ok(inspect) = self.docker.inspect_container(container_name, None).await {
                if let Some(state) = inspect.state {
                    if let Some(running) = state.running {
                        if !running {
                            // Container died! Interrogate logs
                                let logs_opts = bollard::query_parameters::LogsOptions {
                                    stdout: true,
                                    stderr: true,
                                    tail: "1000".to_string(),
                                    ..Default::default()
                                };
                            let mut log_stream = self.docker.logs(container_name, Some(logs_opts));
                            let mut logs = String::new();
                            while let Some(log_res) = log_stream.next().await {
                                if let Ok(l) = log_res {
                                    logs.push_str(&l.to_string());
                                }
                            }
                            let did = container_name.replace("nancy-worker-", "");
                            
                            let entry = self.crash_backoffs.entry(container_name.clone()).or_insert(ContainerState {
                                name: container_name.clone(),
                                failures: 0,
                                next_restart_allowed_at: None,
                            });
                            entry.failures += 1;
                            
                            let base_delay = 5_u64;
                            let max_delay = 300_u64;
                            let mut delay = base_delay * (2_u64.pow((entry.failures - 1).min(6) as u32));
                            delay = std::cmp::min(delay, max_delay);
                            
                            let jitter = rand::thread_rng().gen_range(0..=std::cmp::max(delay / 4, 1));
                            let total_delay = delay + jitter;
                            let now = std::time::Instant::now();
                            entry.next_restart_allowed_at = Some(now + std::time::Duration::from_secs(total_delay));
                            
                            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
                            let log_filename = format!("nancy-worker-{}-crash-{}.log", did, timestamp);
                            
                            crash_reports.push((
                                crate::schema::task::AgentCrashReportPayload {
                                    crashing_agent_did: did.clone(),
                                    log_ref: log_filename,
                                    next_restart_at_unix: Some(timestamp + total_delay),
                                    failures: Some(entry.failures),
                                },
                                logs
                            ));
                            
                            tracing::warn!("Worker {} crashed! failures={} backoff={}s", did, entry.failures, total_delay);
                            
                            let _ = self.docker.remove_container(container_name, Some(bollard::query_parameters::RemoveContainerOptions { force: true, ..Default::default() })).await;
                            to_remove.push(container_name.clone());
                        }
                    }
                }
            } else {
                to_remove.push(container_name.clone());
            }
        }
        
        for name in to_remove {
            self.active_containers.remove(&name);
        }

        for worker in workers {
            let container_name = format!("nancy-worker-{}", worker.did);

            if self.active_containers.contains(&container_name) {
                continue; // Already launched by this coordinator session
            }

            if let Some(state) = self.crash_backoffs.get(&container_name) {
                if let Some(next_at) = state.next_restart_allowed_at {
                    if std::time::Instant::now() < next_at {
                        continue; // Still backing off
                    }
                }
            }

            tracing::info!("Deploying native Hot Grinder {}...", worker.did);

            let nancy_worktrees = self.workdir.join(".nancy").join("worktrees");
            fs::create_dir_all(&nancy_worktrees).await.unwrap_or_default();
            let target_path = nancy_worktrees.join(format!("worker-{}", worker.did));

            if !target_path.exists() {
                let branch_name = format!("refs/heads/nancy/workers/{}", worker.did);
                
                // Natively ensure the target branch exists organically resolving empty repository crashes gracefully!
                if let Ok(repo) = git2::Repository::open(&self.workdir) {
                    if repo.find_reference(&branch_name).is_err() {
                        if let Ok(sig) = git2::Signature::now("Nancy Coordinator", "coordinator@local") {
                            if let Ok(tree_id) = repo.treebuilder(None).and_then(|tb| tb.write()) {
                                if let Ok(tree) = repo.find_tree(tree_id) {
                                    let _ = repo.commit(Some(&branch_name), &sig, &sig, "Init Worker Bounds", &tree, &[]);
                                }
                            }
                        }
                    }
                }

                let shell_cmd = format!(
                    "git worktree prune && git worktree add -f {} {}",
                    target_path.display(), branch_name
                );

                let status = tokio::process::Command::new("sh")
                    .arg("-c")
                    .arg(&shell_cmd)
                    .current_dir(&self.workdir)
                    .status()
                    .await
                    .unwrap_or_else(|e| panic!("Failed executing sh worktree bounds: {}", e));

                if !status.success() {
                    tracing::error!("Failed applying git worktree command natively");
                    continue;
                }
            }

            // Provision socket directory boundaries perfectly natively avoiding Docker Daemon ROOT ownership mapping escalations natively
            let worker_socket_dir = self.workdir.join(".nancy").join("sockets").join(&worker.did);
            fs::create_dir_all(&worker_socket_dir).await.unwrap_or_default();
            let _ = fs::remove_file(worker_socket_dir.join("grinder.sock")).await;
            
            let coordinator_socket_dir = self.workdir.join(".nancy").join("sockets").join("coordinator");
            fs::create_dir_all(&coordinator_socket_dir).await.unwrap_or_default();

            // Provision identity.json explicitly mapping the grinder subset into its context
            let worker_nancy_dir = target_path.join(".nancy");
            fs::create_dir_all(&worker_nancy_dir).await.unwrap_or_default();
            let worker_identity = Identity::Grinder(worker.clone());
            let _ = fs::write(
                worker_nancy_dir.join("identity.json"),
                serde_json::to_string_pretty(&worker_identity).unwrap(),
            ).await;

            // Run container
            let env_vars = build_worker_env_vars(&root_did);
            let config = build_container_config(rt_image, &target_path, &self.workdir, &worker.did, env_vars).await;

            match self.docker
                .create_container(
                    Some(CreateContainerOptions {
                        name: Some(container_name.clone()),
                        platform: "".to_string(),
                    }),
                    config,
                )
                .await
            {
                Ok(response) => {
                    if let Some(ref tar_payload) = cached_tar_payload {
                        let upload_opts = bollard::query_parameters::UploadToContainerOptions {
                            path: "/".to_string(),
                            ..Default::default()
                        };
                        let stream = futures_util::stream::iter(vec![bytes::Bytes::from(tar_payload.clone())]);
                        #[allow(deprecated)]
                        let _ = self.docker.upload_to_container_streaming(&response.id, Some(upload_opts), stream).await;
                    }

                    if let Err(e) = self.docker.start_container(&response.id, None::<StartContainerOptions>).await {
                        tracing::error!("Failed to start container {}: {}", response.id, e);
                    } else {
                        tracing::info!("Container worker {} physically launched.", response.id);
                        // Record running natively to avoid dupes across loop
                        self.active_containers.insert(container_name.clone());
                    }
                }
                Err(e) => {
                    if e.to_string().contains("409") {
                        // Conflict gracefully drops previously orphaned executing boundaries before recreating them
                        tracing::warn!("Container {} orphaned, pruning and re-evaluating gracefully.", container_name);
                        let _ = self.docker.remove_container(&container_name, Some(bollard::query_parameters::RemoveContainerOptions { force: true, ..Default::default() })).await;
                        
                        // We do not silently re-insert if we failed. Usually we'd retry recursively but this loop resets state cleanly on next poll.
                    } else {
                        tracing::error!("Failed to build container node natively: {}", e);
                    }
                }
            }
        }
        
        crash_reports
    }
}

impl Drop for DockerOrchestrator {
    fn drop(&mut self) {
        use bollard::query_parameters::RemoveContainerOptions;
        let docker = self.docker.clone();
        for id in &self.active_containers {
            let container_id = id.clone();
            let docker_clone = docker.clone();
            let _ = std::thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap_or_else(|_| {
                    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
                });
                rt.block_on(async {
                    let opts = RemoveContainerOptions { force: true, ..Default::default() };
                    let _ = docker_clone.remove_container(&container_id, Some(opts)).await;
                });
            });
        }
    }
}
