use git2::Repository;
use nancy::coordinator::appview::AppView;
use nancy::schema::identity_config::Identity;
use std::collections::HashSet;

fn main() {
    let repo = Repository::discover("/home/craig/nancy").unwrap();
    let id_str = std::fs::read_to_string("/home/craig/nancy/.nancy/identity.json").unwrap();
    let identity: Identity = serde_json::from_str(&id_str).unwrap();
    let appview = AppView::hydrate(&repo, &identity, None);
    println!("Assignments: {:#?}", appview.assignments);
    println!("Tasks: {:#?}", appview.tasks.keys());
    println!(
        "Highest Impact Ready: {:#?}",
        appview.get_highest_impact_ready_tasks()
    );
    println!(
        "Target worker: {:#?}",
        match &identity {
            Identity::Coordinator { workers, .. } => workers.first().map(|w| w.did.clone()),
            _ => None,
        }
    );
}
