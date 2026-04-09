use crate::coordinator::appview::AppView;
use git2::Repository;

pub fn ensure_task_branch(repo: &Repository, appview: &AppView, task_id: &String) {
    let task_branch = format!("refs/heads/nancy/tasks/{}", task_id);
    if repo.find_reference(&task_branch).is_ok() {
        return;
    }
    let feature_branch = match appview.get_feature_branch(task_id) {
        Some(b) => b,
        None => return,
    };
    let feat_ref = match repo.find_reference(&feature_branch) {
        Ok(r) => r,
        Err(_) => return,
    };
    if let Ok(commit) = feat_ref.peel_to_commit() {
        let _ = repo.branch(&format!("nancy/tasks/{}", task_id), &commit, false);
    }
}
