//! Tests for DAG command functionality.

use super::helpers::convert_specs;
use super::types::{DagSpecInput, TaskSpecInput};
use cortex_agents::task::{DagHydrator, Task, TaskId, TaskSpec};
use std::collections::HashMap;

use super::executor::TaskExecutor;

#[test]
fn test_load_yaml_spec() {
    let yaml = r#"
name: test-dag
description: Test DAG
tasks:
  - name: setup
    description: Setup environment
  - name: build
    description: Build project
    depends_on:
      - setup
  - name: test
    description: Run tests
    depends_on:
      - build
"#;

    let spec: DagSpecInput = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(spec.name, Some("test-dag".to_string()));
    assert_eq!(spec.tasks.len(), 3);
    assert_eq!(spec.tasks[1].depends_on, vec!["setup"]);
}

#[test]
fn test_convert_specs() {
    let input = DagSpecInput {
        name: Some("test".to_string()),
        description: None,
        tasks: vec![
            TaskSpecInput {
                name: "a".to_string(),
                description: "Task A".to_string(),
                command: Some("echo A".to_string()),
                depends_on: vec![],
                affected_files: vec![],
                priority: 10,
                estimated_duration: None,
                metadata: HashMap::new(),
            },
            TaskSpecInput {
                name: "b".to_string(),
                description: "Task B".to_string(),
                command: None,
                depends_on: vec!["a".to_string()],
                affected_files: vec!["file.txt".to_string()],
                priority: 5,
                estimated_duration: Some(60),
                metadata: HashMap::new(),
            },
        ],
    };

    let specs = convert_specs(&input);
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].priority, 10);
    assert_eq!(specs[1].depends_on, vec!["a"]);
}

#[test]
fn test_dag_creation_with_cycle_detection() {
    let specs = vec![
        TaskSpec::new("a", "A").depends_on("c"),
        TaskSpec::new("b", "B").depends_on("a"),
        TaskSpec::new("c", "C").depends_on("b"),
    ];

    let result = DagHydrator::new().hydrate_from_specs(&specs);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_task_executor_fails_closed_without_sandbox_wrapper() {
    // Library tests run inside the harness, which never registers itself as
    // the sandbox wrapper; a DAG command must not fall back to a bare shell.
    let executor = TaskExecutor::new(30, false);
    let mut task =
        Task::new("test", "Test task").with_metadata("command", serde_json::json!("echo hello"));
    task.id = Some(TaskId::new(1));

    let result = executor.execute(&task).await;
    assert_eq!(result.task_id, TaskId::new(1));
    assert_eq!(result.status, cortex_agents::task::TaskStatus::Failed);
    assert!(result.output.is_none());
    assert!(
        result.error.as_deref().unwrap().contains("unavailable"),
        "{:?}",
        result.error
    );
}

#[tokio::test]
async fn integration_contract_commandless_and_invalid_commands_fail() {
    for command in [
        None,
        Some(serde_json::json!("")),
        Some(serde_json::json!(false)),
    ] {
        let mut task = Task::new("agent", "Do not claim agent execution");
        task.id = Some(TaskId::new(1));
        if let Some(value) = command {
            task.metadata.insert("command".into(), value);
        }
        let result = TaskExecutor::new(1, false).execute(&task).await;
        assert_eq!(result.status, cortex_agents::task::TaskStatus::Failed);
        assert!(result.output.is_none());
        assert!(result.error.is_some());
    }
}

#[tokio::test]
async fn integration_contract_fail_fast_records_failure_before_stopping() {
    use super::scheduler::DagScheduler;
    use super::types::FailureMode;
    use cortex_agents::task::TaskDag;
    for sequential in [false, true] {
        let mut dag = TaskDag::new();
        let first = dag.add_task(Task::new("missing-agent", "Must fail").with_priority(10));
        let dependent = dag.add_task(Task::new("dependent", "Must not run"));
        dag.add_dependency(dependent, first).unwrap();
        dag.add_task(Task::new(
            "independent",
            "Must not be admitted after failure",
        ));
        let scheduler = DagScheduler::new(dag, 1, 1, FailureMode::FailFast, false, true);
        let stats = if sequential {
            scheduler.execute_sequential().await
        } else {
            scheduler.execute().await
        }
        .unwrap();
        assert_eq!(stats.total_tasks, 3);
        assert_eq!(stats.failed_tasks, 1);
        assert_eq!(stats.completed_tasks, 0);
        assert_eq!(stats.skipped_tasks, 2);
        assert_eq!(stats.task_results.len(), 1);
    }
}

#[tokio::test]
async fn integration_contract_continue_never_satisfies_failed_dependencies() {
    use super::scheduler::DagScheduler;
    use super::types::FailureMode;
    use cortex_agents::task::TaskDag;
    let mut dag = TaskDag::new();
    let first = dag.add_task(Task::new("missing-agent", "Must fail"));
    let dependent = dag.add_task(Task::new("dependent", "Must not run"));
    dag.add_dependency(dependent, first).unwrap();
    dag.add_task(Task::new("independent", "Must also fail honestly"));
    let scheduler = DagScheduler::new(dag, 2, 1, FailureMode::Continue, false, true);
    let stats = scheduler.execute().await.unwrap();
    assert_eq!(stats.failed_tasks, 2);
    assert_eq!(stats.skipped_tasks, 1);
    assert_eq!(stats.task_results.len(), 2);
}

#[tokio::test]
async fn integration_contract_rejects_zero_jobs_and_stale_running_tasks() {
    use super::scheduler::DagScheduler;
    use super::types::FailureMode;
    use cortex_agents::task::TaskDag;
    let mut dag = TaskDag::new();
    let id = dag.add_task(Task::new("fixture", "Not executed"));
    assert!(
        DagScheduler::new(dag.clone(), 0, 1, FailureMode::FailFast, false, true)
            .execute()
            .await
            .is_err()
    );
    dag.start_task(id, None).unwrap();
    assert!(
        DagScheduler::new(dag, 1, 1, FailureMode::FailFast, false, true)
            .execute()
            .await
            .is_err()
    );
}

#[tokio::test]
async fn progress_reporting_does_not_change_the_recorded_outcome() {
    use super::scheduler::DagScheduler;
    use super::types::FailureMode;
    use cortex_agents::task::TaskDag;
    // The reporting scheduler (quiet = false) announces starts and failures on
    // the same real execution path; the statistics must be identical.
    let build = || {
        let mut dag = TaskDag::new();
        dag.add_task(Task::new("missing-agent", "Must fail"));
        dag
    };
    let loud = DagScheduler::new(build(), 1, 1, FailureMode::Continue, true, false)
        .execute()
        .await
        .unwrap();
    let quiet = DagScheduler::new(build(), 1, 1, FailureMode::Continue, false, true)
        .execute()
        .await
        .unwrap();
    assert_eq!(loud.total_tasks, 1);
    assert_eq!(loud.failed_tasks, quiet.failed_tasks);
    assert_eq!(loud.completed_tasks, quiet.completed_tasks);
    assert_eq!(loud.skipped_tasks, quiet.skipped_tasks);
    assert_eq!(loud.task_results.len(), quiet.task_results.len());
}

#[tokio::test]
async fn resume_rejects_zero_jobs_and_unknown_dag_identifiers() {
    use super::args::DagResumeArgs;
    use super::commands::run_resume;
    use super::types::{DagOutputFormat, FailureMode};
    let args = |id: &str, jobs: usize| DagResumeArgs {
        id: id.to_string(),
        max_concurrent: jobs,
        timeout: 1,
        failure_mode: FailureMode::FailFast,
        format: DagOutputFormat::Text,
    };
    let zero = run_resume(args("any", 0)).await.unwrap_err().to_string();
    assert!(zero.contains("--jobs"), "{zero}");
    // A DAG that was never created cannot be resumed; this only reads the store.
    let unknown = run_resume(args(
        "cortex-resume-fixture-00000000-0000-0000-0000-000000000000",
        1,
    ))
    .await
    .unwrap_err()
    .to_string();
    assert!(unknown.contains("not found"), "{unknown}");
}

#[tokio::test]
async fn integration_contract_command_exit_and_timeout_are_not_success() {
    use cortex_agents::task::TaskStatus;
    for command in ["exit 7", "sleep 5"] {
        let mut task = Task::new("fixture", "Controlled shell task")
            .with_metadata("command", serde_json::json!(command));
        task.id = Some(TaskId::new(1));
        let start = std::time::Instant::now();
        let result = TaskExecutor::new(1, false).execute(&task).await;
        assert_eq!(result.status, TaskStatus::Failed);
        assert!(start.elapsed() < std::time::Duration::from_secs(4));
    }
}
