use crate::schema::registry::EventPayload;
use crate::schema::identity_config::Identity;
use crate::events::reader::Reader;
use std::collections::{HashMap, HashSet};
use git2::Repository;

pub struct AppView {
    pub tasks: HashMap<String, EventPayload>,
    pub requests: HashMap<String, EventPayload>,
    pub handled_requests: HashSet<String>,
    pub blocked_by: HashMap<String, HashSet<String>>,
    pub task_completions: HashSet<String>,
    pub completed_reports: HashMap<String, String>,
    pub assignments: HashMap<String, String>, // task_ref -> assignee_did
    pub assignment_targets: HashMap<String, String>, // assignment_ref -> task_ref
    pub agent_crashes: HashMap<String, crate::schema::task::AgentCrashReportPayload>,
    pub task_evaluations: HashMap<String, crate::schema::task::TaskEvaluationPayload>, // evaluated_event_id -> payload
    pub active_asks: HashMap<String, crate::schema::task::AskPayload>,
}

impl AppView {
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            requests: HashMap::new(),
            handled_requests: HashSet::new(),
            blocked_by: HashMap::new(),
            task_completions: HashSet::new(),
            completed_reports: HashMap::new(),
            assignments: HashMap::new(),
            assignment_targets: HashMap::new(),
            agent_crashes: HashMap::new(),
            task_evaluations: HashMap::new(),
            active_asks: HashMap::new(),
        }
    }

    pub fn hydrate(repo: &Repository, identity: &Identity, restricted_grinder_did: Option<&str>) -> Self {
        let mut appview = Self::new();
        let did = identity.get_did_owner().did.clone();

        // Poll our own ledger to hydrate AppView structurally
        let reader = Reader::new(repo, did);
        if let Ok(iter) = reader.iter_events() {
            for ev_res in iter {
                if let Ok(env) = ev_res {
                    appview.apply_event(&env.payload, &env.id);
                }
            }
        }

        // Sync with grinders to receive their AssignmentCompletes
        if let Identity::Coordinator { workers, .. } = identity {
            for worker in workers {
                if let Some(r_did) = restricted_grinder_did {
                    if worker.did != r_did {
                        continue;
                    }
                }
                let local_reader = Reader::new(repo, worker.did.clone());
                if let Ok(iter) = local_reader.iter_events() {
                    for ev_res in iter {
                        if let Ok(env) = ev_res {
                            appview.apply_event(&env.payload, &env.id);
                        }
                    }
                }
            }
        }

        appview
    }

    pub fn apply_event(&mut self, payload: &EventPayload, event_id: &str) {
        match payload {
            EventPayload::Task(_) => {
                self.tasks.insert(event_id.to_string(), payload.clone());
            }
            EventPayload::TaskRequest(_) => {
                self.requests.insert(event_id.to_string(), payload.clone());
            }
            EventPayload::BlockedBy(b) => {
                if self.requests.contains_key(&b.target) {
                    self.handled_requests.insert(b.target.clone());
                }
                self.blocked_by
                    .entry(b.target.clone())
                    .or_default()
                    .insert(b.source.clone());
            }
            EventPayload::AssignmentComplete(c) => {
                if let Some(target_ref) = self.assignment_targets.get(&c.assignment_ref) {
                    self.assignments.remove(target_ref);
                    self.task_completions.insert(target_ref.clone());
                    self.completed_reports
                        .insert(target_ref.clone(), c.report.clone());
                }
            }
            EventPayload::CoordinatorAssignment(a) => {
                let crate::schema::task::CoordinatorAssignmentPayload {
                    task_ref,
                    assignee_did,
                } = a;
                self.assignments
                    .insert(task_ref.clone(), assignee_did.clone());
                self.assignment_targets
                    .insert(event_id.to_string(), task_ref.clone());
            }
            EventPayload::AgentCrashReport(p) => {
                self.agent_crashes.insert(p.crashing_agent_did.clone(), p.clone());
            }
            EventPayload::TaskEvaluation(e) => {
                self.task_evaluations.insert(e.evaluated_event_id.clone(), e.clone());
            }
            EventPayload::Ask(a) => {
                self.active_asks.insert(a.ask_ref.clone(), a.clone());
            }
            EventPayload::CancelAsk(c) => {
                self.active_asks.remove(&c.ask_ref);
            }
            EventPayload::HumanResponse(hr) => {
                self.active_asks.remove(&hr.ask_ref);
            }
            _ => {}
        }
    }

    pub fn get_pagerank_scores(&self) -> HashMap<String, f64> {
        let mut pr = HashMap::new();
        let damping = 0.85;
        let tasks: Vec<&String> = self
            .tasks
            .keys()
            .filter(|id| {
                !self.task_completions.contains(*id) && !self.assignments.contains_key(*id)
            })
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
                        out_degree
                            .entry((*t).clone())
                            .and_modify(|e| *e += 1)
                            .or_insert(1);
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
            let has_blocker = self
                .blocked_by
                .get(task_id)
                .map(|blockers| blockers.iter().any(|b| !self.task_completions.contains(b)))
                .unwrap_or(false);

            if !has_blocker {
                ready.push((task_id.clone(), *score));
            }
        }

        ready.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ready.into_iter().map(|(id, _)| id).collect()
    }

    pub fn get_feature_branch(&self, task_id: &str) -> Option<String> {
        let mut visited = HashSet::new();
        let mut queue = vec![task_id.to_string()];

        while let Some(current) = queue.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(EventPayload::Task(t)) = self.tasks.get(&current) {
                if matches!(t.action, crate::schema::task::TaskAction::Plan) {
                    return Some(format!("refs/heads/nancy/features/{}", current));
                }
            }
            if let Some(blockers) = self.blocked_by.get(&current) {
                queue.extend(blockers.iter().cloned());
            }
        }
        None
    }

    pub fn get_implement_task_id(&self, review_task_id: &str) -> Option<String> {
        if let Some(blockers) = self.blocked_by.get(review_task_id) {
            for b in blockers {
                if let Some(EventPayload::Task(t)) = self.tasks.get(b) {
                    if matches!(t.action, crate::schema::task::TaskAction::Implement) {
                        return Some(b.clone());
                    }
                }
            }
        }
        None
    }

    pub fn get_topology(&self) -> crate::schema::web::TopologyResponse {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for (id, payload) in &self.requests {
            if let EventPayload::TaskRequest(r) = payload {
                nodes.push(crate::schema::web::TopologyNode {
                    id: id.clone(),
                    node_type: crate::schema::web::NodeType::TaskRequest,
                    name: r.description.clone(),
                    active_agent: None,
                    is_completed: self.handled_requests.contains(id),
                    x: 0.0,
                    y: 0.0,
                });
            }
        }

        for (id, payload) in &self.tasks {
            if let EventPayload::Task(t) = payload {
                let node_type = if t.action == crate::schema::task::TaskAction::Plan {
                    crate::schema::web::NodeType::Plan
                } else {
                    crate::schema::web::NodeType::Task
                };
                
                let active_agent = self.assignments.get(id).cloned();
                let is_completed = self.task_completions.contains(id);

                nodes.push(crate::schema::web::TopologyNode {
                    id: id.clone(),
                    node_type,
                    name: t.description.clone(),
                    active_agent,
                    is_completed,
                    x: 0.0,
                    y: 0.0,
                });
            }
        }

        for (target, sources) in &self.blocked_by {
            for source in sources {
                edges.push(crate::schema::web::TopologyEdge {
                    source: source.clone(),
                    target: target.clone(),
                    points: Vec::new(),
                });
            }
        }

        for (id, payload) in &self.active_asks {
            nodes.push(crate::schema::web::TopologyNode {
                id: id.clone(),
                node_type: crate::schema::web::NodeType::Ask,
                name: payload.question.clone(),
                active_agent: Some(payload.agent_path.clone()),
                is_completed: false,
                x: 0.0,
                y: 0.0,
            });
        }

        // Layout evaluation
        use dugong::graphlib::{Graph, GraphOptions};
        use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir, layout};

        let mut g: Graph<NodeLabel, EdgeLabel, GraphLabel> = Graph::new(GraphOptions {
            multigraph: true,
            compound: true,
            directed: true,
        });

        g.set_graph(GraphLabel {
            rankdir: RankDir::TB,
            nodesep: 60.0,
            ranksep: 100.0,
            ..Default::default()
        });

        g.set_default_edge_label(EdgeLabel::default);

        const NODE_WIDTH: f64 = 280.0;
        const NODE_HEIGHT: f64 = 80.0;

        for node in &nodes {
            g.set_node(&node.id, NodeLabel {
                width: NODE_WIDTH,
                height: NODE_HEIGHT,
                ..Default::default()
            });
        }

        for edge in &edges {
            if g.has_node(&edge.source) && g.has_node(&edge.target) {
                g.set_edge(&edge.source, &edge.target);
            }
        }

        layout(&mut g);

        let mut max_width = 0.0_f64;
        let mut max_height = 0.0_f64;

        for node in &mut nodes {
            if let Some(n) = g.node(&node.id) {
                let nx = n.x.unwrap_or(0.0);
                let ny = n.y.unwrap_or(0.0);
                node.x = nx;
                node.y = ny;
                max_width = max_width.max(nx + NODE_WIDTH + 200.0);
                max_height = max_height.max(ny + NODE_HEIGHT + 200.0);
            }
        }

        for edge in &mut edges {
            if g.has_node(&edge.source) && g.has_node(&edge.target) {
                if let Some(e) = g.edge(&edge.source, &edge.target, None) {
                    edge.points = e.points.iter().map(|p| (p.x, p.y)).collect();
                }
            }
        }

        // Sort for deterministic layout stability organically
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        edges.sort_by(|a, b| a.source.cmp(&b.source).then(a.target.cmp(&b.target)));

        crate::schema::web::TopologyResponse {
            version: 0,
            max_width,
            max_height,
            nodes,
            edges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::task::{
        BlockedByPayload, TaskAction,
        TaskPayload, TaskRequestPayload,
    };

    #[test]
    fn test_appview_request_task_separation() {
        let mut appview = AppView::new();

        appview.apply_event(
            &EventPayload::TaskRequest(TaskRequestPayload {
                requestor: "Alice".to_string(),
                description: "Feature foo".to_string(),
            }),
            "req-id",
        );

        let task_event = EventPayload::Task(TaskPayload {
            description: "Some action".into(),
            preconditions: "none".into(),
            postconditions: "done".into(),
            validation_strategy: "noop".into(),
            action: TaskAction::Plan,
            branch: "refs/heads/nancy/plans/test".into(),
            plan: None,
        });
        appview.apply_event(&task_event, "task-id");

        assert_eq!(appview.requests.len(), 1);
        assert_eq!(appview.tasks.len(), 1);
        assert!(appview.requests.contains_key("req-id"));
        assert!(appview.tasks.contains_key("task-id"));
    }

    #[test]
    fn test_appview_feature_branch_traversal() {
        let mut appview = AppView::new();

        let review_plan = EventPayload::Task(TaskPayload {
            description: "Review plan".into(),
            preconditions: "".into(),
            postconditions: "".into(),
            validation_strategy: "".into(),
            action: TaskAction::Plan,
            branch: "refs/heads/nancy/tasks/review-id".into(),
            plan: None,
        });

        // This task depends on ReviewPlan
        let child_task = EventPayload::Task(TaskPayload {
            description: "Implement task".into(),
            preconditions: "".into(),
            postconditions: "".into(),
            validation_strategy: "".into(),
            action: TaskAction::Implement,
            branch: "refs/heads/nancy/tasks/impl-id".into(),
            plan: None,
        });

        appview.apply_event(&review_plan, "review-id");
        appview.apply_event(&child_task, "impl-id");

        appview.apply_event(
            &EventPayload::BlockedBy(BlockedByPayload {
                source: "review-id".to_string(),
                target: "impl-id".to_string(),
            }),
            "bb1",
        );

        let feature_branch = appview.get_feature_branch("impl-id");
        assert_eq!(
            feature_branch,
            Some("refs/heads/nancy/features/review-id".to_string())
        );
    }

    #[test]
    fn test_appview_implement_task_lookup() {
        let mut appview = AppView::new();

        let implement_task = EventPayload::Task(TaskPayload {
            description: "Worker".into(),
            preconditions: "".into(),
            postconditions: "".into(),
            validation_strategy: "".into(),
            action: TaskAction::Implement,
            branch: "refs/heads/nancy/tasks/impl-id".into(),
            plan: None,
        });

        let review_impl = EventPayload::Task(TaskPayload {
            description: "Review worker".into(),
            preconditions: "".into(),
            postconditions: "".into(),
            validation_strategy: "".into(),
            action: TaskAction::ReviewImplementation,
            branch: "refs/heads/nancy/tasks/review-impl-id".into(),
            plan: None,
        });

        appview.apply_event(&implement_task, "impl-id");
        appview.apply_event(&review_impl, "review-impl-id");

        appview.apply_event(
            &EventPayload::BlockedBy(BlockedByPayload {
                source: "impl-id".to_string(),
                target: "review-impl-id".to_string(),
            }),
            "bb2",
        );

        let implement_target = appview.get_implement_task_id("review-impl-id");
        assert_eq!(implement_target, Some("impl-id".to_string()));
    }

    #[test]
    fn test_pagerank_highest_impact() {
        let mut view = AppView::new();

        let default_fields = || ("none".to_string(), "none".to_string(), "none".to_string());

        // 3 tasks. T1 blocks T2 and T3.
        view.apply_event(
            &EventPayload::Task(TaskPayload {
                description: "T1".into(),
                preconditions: default_fields().0,
                postconditions: default_fields().1,
                validation_strategy: default_fields().2,
                action: crate::schema::task::TaskAction::Implement,
                branch: "refs/heads/nancy/tasks/t1".into(),
                plan: None,
            }),
            "t1",
        );
        view.apply_event(
            &EventPayload::Task(TaskPayload {
                description: "T2".into(),
                preconditions: default_fields().0,
                postconditions: default_fields().1,
                validation_strategy: default_fields().2,
                action: crate::schema::task::TaskAction::Implement,
                branch: "refs/heads/nancy/tasks/t2".into(),
                plan: None,
            }),
            "t2",
        );
        view.apply_event(
            &EventPayload::Task(TaskPayload {
                description: "T3".into(),
                preconditions: default_fields().0,
                postconditions: default_fields().1,
                validation_strategy: default_fields().2,
                action: crate::schema::task::TaskAction::Implement,
                branch: "refs/heads/nancy/tasks/t3".into(),
                plan: None,
            }),
            "t3",
        );

        view.apply_event(
            &EventPayload::BlockedBy(crate::schema::task::BlockedByPayload {
                source: "t1".into(),
                target: "t2".into(),
            }),
            "e1",
        );
        view.apply_event(
            &EventPayload::BlockedBy(crate::schema::task::BlockedByPayload {
                source: "t1".into(),
                target: "t3".into(),
            }),
            "e2",
        );

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0], "t1"); // T1 is the only ready task, and has highest impact!

        // Assign T1
        view.apply_event(
            &EventPayload::CoordinatorAssignment(
                crate::schema::task::CoordinatorAssignmentPayload {
                    task_ref: "t1".into(),
                    assignee_did: "worker".into(),
                },
            ),
            "assignment_event_id",
        );

        // Complete T1 assignment
        view.apply_event(
            &EventPayload::AssignmentComplete(crate::schema::task::AssignmentCompletePayload {
                assignment_ref: "assignment_event_id".into(),
                report: "done".into(),
            }),
            "e3",
        );

        // Assign task
        view.apply_event(
            &EventPayload::CoordinatorAssignment(
                crate::schema::task::CoordinatorAssignmentPayload {
                    task_ref: "req1".into(),
                    assignee_did: "worker".into(),
                },
            ),
            "plan_event_id",
        );
        assert_eq!(view.assignments.get("req1").unwrap(), "worker");
        assert_eq!(
            view.assignment_targets.get("plan_event_id").unwrap(),
            "req1"
        );

        let ready = view.get_highest_impact_ready_tasks();
        assert_eq!(ready.len(), 2); // T2 and T3 are now ready
    }
}
