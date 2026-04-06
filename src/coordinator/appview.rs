use std::collections::{HashMap, HashSet};
use crate::schema::registry::EventPayload;

pub struct AppView {
    pub tasks: HashMap<String, EventPayload>,
    pub blocked_by: HashMap<String, HashSet<String>>,
    pub task_completions: HashSet<String>,
    pub assignments: HashMap<String, String>, // task_ref -> assignee_did
    pub assignment_targets: HashMap<String, String>, // assignment_ref -> task_ref
}

impl AppView {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            blocked_by: HashMap::new(),
            task_completions: HashSet::new(),
            assignments: HashMap::new(),
            assignment_targets: HashMap::new(),
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
            EventPayload::AssignmentComplete(c) => {
                if let Some(target_ref) = self.assignment_targets.get(&c.assignment_ref) {
                    self.assignments.remove(target_ref);
                    self.task_completions.insert(target_ref.clone());
                }
            }
            EventPayload::CoordinatorAssignment(a) => {
                use crate::schema::task::CoordinatorAssignmentPayload;
                match a {
                    CoordinatorAssignmentPayload::PlanTask { task_request_ref, assignee_did } => {
                        self.assignments.insert(task_request_ref.clone(), assignee_did.clone());
                        self.assignment_targets.insert(event_id.to_string(), task_request_ref.clone());
                    }
                    CoordinatorAssignmentPayload::PerformTask { task_ref, assignee_did } => {
                        self.assignments.insert(task_ref.clone(), assignee_did.clone());
                        self.assignment_targets.insert(event_id.to_string(), task_ref.clone());
                    }
                }
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

        let default_fields = || {
            ("none".to_string(), "none".to_string(), "none".to_string())
        };

        // 3 tasks. T1 blocks T2 and T3.
        view.apply_event(&EventPayload::Task(TaskPayload { description: "T1".into(), preconditions: default_fields().0, postconditions: default_fields().1, validation_strategy: default_fields().2 }), "t1");
        view.apply_event(&EventPayload::Task(TaskPayload { description: "T2".into(), preconditions: default_fields().0, postconditions: default_fields().1, validation_strategy: default_fields().2 }), "t2");
        view.apply_event(&EventPayload::Task(TaskPayload { description: "T3".into(), preconditions: default_fields().0, postconditions: default_fields().1, validation_strategy: default_fields().2 }), "t3");

        view.apply_event(&EventPayload::BlockedBy(crate::schema::task::BlockedByPayload { source: "t2".into(), target: "t1".into() }), "e1");
        view.apply_event(&EventPayload::BlockedBy(crate::schema::task::BlockedByPayload { source: "t3".into(), target: "t1".into() }), "e2");

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "t1"); // T1 is the only ready task, and has highest impact!

        // Assign T1
        view.apply_event(&EventPayload::CoordinatorAssignment(crate::schema::task::CoordinatorAssignmentPayload::PerformTask { task_ref: "t1".into(), assignee_did: "worker".into() }), "assignment_event_id");

        // Complete T1 assignment
        view.apply_event(&EventPayload::AssignmentComplete(crate::schema::task::AssignmentCompletePayload { assignment_ref: "assignment_event_id".into(), report: "done".into() }), "e3");
        
        // Assign PlanTask
        view.apply_event(&EventPayload::CoordinatorAssignment(crate::schema::task::CoordinatorAssignmentPayload::PlanTask { task_request_ref: "req1".into(), assignee_did: "worker".into() }), "plan_event_id");
        assert_eq!(view.assignments.get("req1").unwrap(), "worker");
        assert_eq!(view.assignment_targets.get("plan_event_id").unwrap(), "req1");

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 2); // T2 and T3 are now ready
    }
}
