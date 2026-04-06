use std::collections::{HashMap, HashSet};
use crate::schema::registry::EventPayload;

pub struct AppView {
    pub tasks: HashMap<String, EventPayload>,
    pub blocked_by: HashMap<String, HashSet<String>>,
    pub task_completions: HashSet<String>,
    pub assignments: HashMap<String, String>,
}

impl AppView {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            blocked_by: HashMap::new(),
            task_completions: HashSet::new(),
            assignments: HashMap::new(),
        }
    }

    pub fn apply_event(&mut self, payload: &EventPayload, event_id: &str) {
        match payload {
            EventPayload::Task(_) => {
                self.tasks.insert(event_id.to_string(), payload.clone());
            }
            EventPayload::BlockedBy(b) => {
                self.blocked_by.entry(b.source.clone()).or_default().insert(b.target.clone());
            }
            EventPayload::TaskComplete(c) => {
                self.task_completions.insert(c.task_ref.clone());
            }
            EventPayload::TaskAssigned(a) => {
                self.assignments.insert(a.task_ref.clone(), a.assignee_did.clone());
            }
            _ => {}
        }
    }

    pub fn get_pagerank_scores(&self) -> HashMap<String, f64> {
        let mut pr = HashMap::new();
        let damping = 0.85;
        let tasks: Vec<&String> = self.tasks.keys()
            .filter(|id| !self.task_completions.contains(*id) && !self.assignments.contains_key(*id))
            .collect();
            
        let n = tasks.len() as f64;
        if n == 0.0 {
            return pr;
        }

        for t in &tasks {
            pr.insert((*t).clone(), 1.0 / n);
        }

        let mut out_degree: HashMap<String, usize> = HashMap::new();
        let mut edges: HashMap<String, Vec<String>> = HashMap::new();

        for t in &tasks {
            if let Some(targets) = self.blocked_by.get(*t) {
                for target in targets {
                    if !self.task_completions.contains(target) {
                        out_degree.entry((*t).clone()).and_modify(|e| *e += 1).or_insert(1);
                        edges.entry((*t).clone()).or_default().push(target.clone());
                    }
                }
            }
        }

        for _ in 0..20 {
            let mut new_pr = HashMap::new();
            for t in &tasks {
                new_pr.insert((*t).clone(), (1.0 - damping) / n);
            }

            for t in &tasks {
                if let Some(targets) = edges.get(*t) {
                    let share = pr[*t] / out_degree[*t] as f64;
                    for target in targets {
                        if new_pr.contains_key(target) {
                            *new_pr.get_mut(target).unwrap() += damping * share;
                        }
                    }
                } else {
                    let share = (pr[*t] * damping) / n;
                    for other in &tasks {
                        *new_pr.get_mut(*other).unwrap() += share;
                    }
                }
            }
            pr = new_pr;
        }
        
        pr
    }

    pub fn get_highest_impact_ready_tasks(&self) -> Vec<String> {
        let scores = self.get_pagerank_scores();
        
        let mut ready = Vec::new();
        for (task_id, score) in scores.iter() {
            let has_blocker = self.blocked_by.get(task_id).map(|blockers| {
                blockers.iter().any(|b| !self.task_completions.contains(b))
            }).unwrap_or(false);
            
            if !has_blocker {
                ready.push((task_id.clone(), *score));
            }
        }

        ready.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ready.into_iter().map(|(id, _)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::task::TaskPayload;

    #[test]
    fn test_pagerank_highest_impact() {
        let mut view = AppView::new();

        // 3 tasks. T1 blocks T2 and T3.
        view.apply_event(&EventPayload::Task(TaskPayload { requestor: "A".into(), description: "T1".into() }), "t1");
        view.apply_event(&EventPayload::Task(TaskPayload { requestor: "A".into(), description: "T2".into() }), "t2");
        view.apply_event(&EventPayload::Task(TaskPayload { requestor: "A".into(), description: "T3".into() }), "t3");

        view.apply_event(&EventPayload::BlockedBy(crate::schema::registry::BlockedByPayload { source: "t2".into(), target: "t1".into() }), "e1");
        view.apply_event(&EventPayload::BlockedBy(crate::schema::registry::BlockedByPayload { source: "t3".into(), target: "t1".into() }), "e2");

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "t1"); // T1 is the only ready task, and has highest impact!

        // Complete T1
        view.apply_event(&EventPayload::TaskComplete(crate::schema::registry::TaskCompletePayload { task_ref: "t1".into(), commit_sha: "c1".into() }), "e3");

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 2); // T2 and T3 are now ready
    }
}
