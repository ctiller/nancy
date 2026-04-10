use std::fs;

fn main() {
    let files = [
        "src/commands/grind.rs",
        "src/events/writer.rs",
        "src/tasks/manager.rs",
        "src/coordinator/workflow.rs",
        "tests/unified_dag_e2e.rs",
        "src/eval/mod.rs",
    ];

    for file in files.iter() {
        if let Ok(content) = fs::read_to_string(file) {
            let mut result = String::new();
            let mut insert_next = false;
            for line in content.lines() {
                result.push_str(line);
                result.push('\n');
                if line.contains("workers:") && (line.contains("vec![]") || line.contains("workers.clone()")) {
                    if file.contains("unified_dag_e2e.rs") {
                        result.push_str("            dreamer: nancy::schema::identity_config::DidOwner::generate(),\n");
                    } else {
                        result.push_str("            dreamer: crate::schema::identity_config::DidOwner::generate(),\n");
                    }
                }
            }
            fs::write(file, result).unwrap();
        }
    }
}
