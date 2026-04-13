use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::watch;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SerializedElement {
    Log {
        message: String,
    },
    Data {
        key: String,
        value: serde_json::Value,
    },
    Frame(SerializedFrame),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedFrame {
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub rollup: Option<String>,
    pub elements: Vec<SerializedElement>,
}

#[derive(Clone)]
pub enum StateElement {
    Log(String),
    Data(String, serde_json::Value),
    Frame(FrameNode),
}

impl StateElement {
    pub fn snapshot(&self) -> SerializedElement {
        self.snapshot_depth(usize::MAX)
    }

    pub fn snapshot_depth(&self, depth: usize) -> SerializedElement {
        match self {
            StateElement::Log(m) => SerializedElement::Log { message: m.clone() },
            StateElement::Data(k, v) => SerializedElement::Data {
                key: k.clone(),
                value: v.clone(),
            },
            StateElement::Frame(f) => SerializedElement::Frame(f.snapshot_depth(depth)),
        }
    }
}

#[derive(Clone)]
pub struct FrameNode {
    pub name: String,
    pub status: Arc<Mutex<Option<String>>>,
    pub rollup: Arc<Mutex<Option<String>>>,
    pub elements: Arc<Mutex<Vec<StateElement>>>,
}

impl FrameNode {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            status: Arc::new(Mutex::new(None)),
            rollup: Arc::new(Mutex::new(None)),
            elements: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn snapshot(&self) -> SerializedFrame {
        self.snapshot_depth(usize::MAX)
    }

    pub fn snapshot_depth(&self, depth: usize) -> SerializedFrame {
        let status = self.status.lock().unwrap().clone();
        let rollup = self.rollup.lock().unwrap().clone();
        let elements = if depth == 0 {
            Vec::new() // Omit elements at depth 0
        } else {
            let elements_lock = self.elements.lock().unwrap();
            elements_lock
                .iter()
                .map(|e| e.snapshot_depth(depth - 1))
                .collect()
        };

        SerializedFrame {
            name: self.name.clone(),
            status,
            rollup,
            elements,
        }
    }

    pub fn find_frame_by_path(&self, path: &[String]) -> Option<FrameNode> {
        if path.is_empty() {
            return Some(self.clone());
        }
        if path[0] == self.name {
            if path.len() == 1 {
                return Some(self.clone());
            }
            let elements = self.elements.lock().unwrap();
            for el in elements.iter() {
                if let StateElement::Frame(child) = el {
                    if let Some(found) = child.find_frame_by_path(&path[1..]) {
                        return Some(found);
                    }
                }
            }
        } else if self.name == "root" && path[0] != "root" {
            // Implicitly allow paths starting below root when searching from root
            let elements = self.elements.lock().unwrap();
            for el in elements.iter() {
                if let StateElement::Frame(child) = el {
                    if let Some(found) = child.find_frame_by_path(path) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }
}

pub struct IntrospectionContext {
    pub current_frame: FrameNode,
    pub updater: watch::Sender<u64>,
}

tokio::task_local! {
    pub static INTROSPECTION_CTX: IntrospectionContext;
}

pub fn log(message: &str) {
    let _ = INTROSPECTION_CTX.try_with(|ctx| {
        ctx.current_frame
            .elements
            .lock()
            .unwrap()
            .push(StateElement::Log(message.to_string()));
        let _ = ctx.updater.send_modify(|v| *v += 1);
    });
}

pub fn data_log(key: &str, value: serde_json::Value) {
    let _ = INTROSPECTION_CTX.try_with(|ctx| {
        ctx.current_frame
            .elements
            .lock()
            .unwrap()
            .push(StateElement::Data(key.to_string(), value));
        let _ = ctx.updater.send_modify(|v| *v += 1);
    });
}

pub fn set_frame_status(status: &str) {
    let _ = INTROSPECTION_CTX.try_with(|ctx| {
        *ctx.current_frame.status.lock().unwrap() = Some(status.to_string());
        let _ = ctx.updater.send_modify(|v| *v += 1);
    });
}

pub async fn frame<Fut, R>(name: &str, fut: Fut) -> R
where
    Fut: std::future::Future<Output = R>,
{
    // If not in context, just run the future.
    let ctx_opt = INTROSPECTION_CTX.try_with(|ctx| {
        let child_node = FrameNode::new(name);
        ctx.current_frame
            .elements
            .lock()
            .unwrap()
            .push(StateElement::Frame(child_node.clone()));
        let _ = ctx.updater.send_modify(|v| *v += 1);

        IntrospectionContext {
            current_frame: child_node,
            updater: ctx.updater.clone(),
        }
    });

    match ctx_opt {
        Ok(new_ctx) => INTROSPECTION_CTX.scope(new_ctx, fut).await,
        Err(_) => fut.await,
    }
}

pub struct IntrospectionTreeRoot {
    pub root_frame: FrameNode,
    pub updater: watch::Sender<u64>,
    pub receiver: watch::Receiver<u64>,
}

impl IntrospectionTreeRoot {
    pub fn new() -> Self {
        let (updater, receiver) = watch::channel(0);
        Self {
            root_frame: FrameNode::new("root"),
            updater,
            receiver,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_introspection_tree_mechanics() {
        let tree_root = IntrospectionTreeRoot::new();
        let ctx = IntrospectionContext {
            current_frame: tree_root.root_frame.clone(),
            updater: tree_root.updater.clone(),
        };

        // Snapshot initially
        let snap = tree_root.root_frame.snapshot();
        assert_eq!(snap.name, "root");
        assert!(snap.elements.is_empty());

        let rx = tree_root.receiver.clone();
        assert_eq!(*rx.borrow(), 0);

        INTROSPECTION_CTX
            .scope(ctx, async {
                log("hello world");
                data_log("my_key", serde_json::json!({"foo": "bar"}));

                frame("child_task", async {
                    log("inside child");
                })
                .await;
            })
            .await;

        assert_eq!(*rx.borrow(), 4); // 4 updates total

        let snap2 = tree_root.root_frame.snapshot();
        assert_eq!(snap2.elements.len(), 3);

        match &snap2.elements[0] {
            SerializedElement::Log { message } => assert_eq!(message, "hello world"),
            _ => panic!("Expected log"),
        }

        match &snap2.elements[1] {
            SerializedElement::Data { key, value } => {
                assert_eq!(key, "my_key");
                assert_eq!(value, &serde_json::json!({"foo": "bar"}));
            }
            _ => panic!("Expected data"),
        }

        match &snap2.elements[2] {
            SerializedElement::Frame(f) => {
                assert_eq!(f.name, "child_task");
                assert_eq!(f.elements.len(), 1);
                match &f.elements[0] {
                    SerializedElement::Log { message } => assert_eq!(message, "inside child"),
                    _ => panic!("Expected child log"),
                }
            }
            _ => panic!("Expected frame"),
        }
    }

    #[tokio::test]
    async fn test_frame_outside_context_runs_but_does_not_panic() {
        let res = frame("orphaned", async { 42 }).await;
        assert_eq!(res, 42);
        // logs outside should not panic (they use try_with and swallow Err)
        log("orphaned log");
        data_log("k", serde_json::json!(1));
    }
}
