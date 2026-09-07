//! DAG scheduler with bounded admission and a single owner of task state.

use anyhow::{Context, Result, bail};
use cortex_agents::task::{Task, TaskDag, TaskStatus};
use futures::{FutureExt, StreamExt, stream::FuturesUnordered};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

use crate::styled_output::{print_error, print_info};

use super::executor::TaskExecutor;
use super::types::{DagExecutionStats, FailureMode, TaskExecutionResult};

pub struct DagScheduler {
    pub dag: Arc<RwLock<TaskDag>>,
    executor: Arc<TaskExecutor>,
    max_concurrent: usize,
    failure_mode: FailureMode,
    stats: Mutex<DagExecutionStats>,
    quiet: bool,
}

impl DagScheduler {
    pub fn new(
        dag: TaskDag,
        max_concurrent: usize,
        timeout_secs: u64,
        failure_mode: FailureMode,
        verbose: bool,
        quiet: bool,
    ) -> Self {
        Self {
            dag: Arc::new(RwLock::new(dag)),
            executor: Arc::new(TaskExecutor::new(timeout_secs, verbose)),
            max_concurrent,
            failure_mode,
            stats: Mutex::new(DagExecutionStats::default()),
            quiet,
        }
    }

    pub async fn execute(&self) -> Result<DagExecutionStats> {
        self.run(self.max_concurrent).await
    }

    pub async fn execute_sequential(&self) -> Result<DagExecutionStats> {
        self.run(1).await
    }

    async fn run(&self, concurrency: usize) -> Result<DagExecutionStats> {
        if concurrency == 0 || self.max_concurrent == 0 {
            bail!("--jobs must be at least 1");
        }
        let start = Instant::now();
        {
            let dag = self.dag.read().await;
            dag.topological_sort().context("DAG validation failed")?;
            if dag
                .all_tasks()
                .any(|task| task.status == TaskStatus::Running)
            {
                bail!("DAG contains interrupted running tasks; reconcile them before resuming");
            }
        }
        *self.stats.lock().await = DagExecutionStats::default();
        // Futures are owned here, never detached on scheduler cancellation.
        let mut running = FuturesUnordered::new();
        let mut stop = false;
        loop {
            if !stop {
                for task in self.take_ready(concurrency - running.len()).await? {
                    running.push(self.execute_task(task));
                }
            }
            let Some(result) = running.next().await else {
                break;
            };
            let failed = result.status == TaskStatus::Failed;
            self.record_result(result).await?;
            // Observe each failure before admitting more tasks. Already running
            // work drains through its normal execution owner's timeout/cleanup.
            stop |= failed && matches!(self.failure_mode, FailureMode::FailFast);
        }
        self.finish(start, stop).await
    }

    async fn take_ready(&self, limit: usize) -> Result<Vec<Task>> {
        let mut dag = self.dag.write().await;
        let ready: Vec<Task> = dag
            .get_ready_tasks_by_priority()
            .into_iter()
            .take(limit)
            .cloned()
            .collect();
        for task in &ready {
            let id = task.id.context("DAG task has no ID")?;
            dag.start_task(id, None)?;
            if !self.quiet {
                print_info(&format!("Starting task {id}"));
            }
        }
        Ok(ready)
    }

    async fn execute_task(&self, task: Task) -> TaskExecutionResult {
        let start = Instant::now();
        match AssertUnwindSafe(self.executor.execute(&task))
            .catch_unwind()
            .await
        {
            Ok(result) => result,
            Err(_) => TaskExecutionResult {
                task_id: task.id.expect("Admitted task has an ID"),
                task_name: task.name,
                status: TaskStatus::Failed,
                duration: start.elapsed(),
                output: None,
                error: Some("Task execution failed unexpectedly".into()),
            },
        }
    }

    async fn record_result(&self, result: TaskExecutionResult) -> Result<()> {
        let mut dag = self.dag.write().await;
        match result.status {
            TaskStatus::Completed => dag.complete_task(result.task_id, result.output.clone())?,
            TaskStatus::Failed => {
                dag.fail_task(result.task_id, result.error.clone().unwrap_or_default())?;
                if !self.quiet {
                    print_error(&format!("Task {} failed", result.task_id));
                }
            }
            _ => bail!("Task executor returned a nonterminal result"),
        }
        self.stats.lock().await.task_results.push(result);
        Ok(())
    }

    async fn finish(&self, start: Instant, stopped: bool) -> Result<DagExecutionStats> {
        let mut dag = self.dag.write().await;
        if stopped {
            let pending: Vec<_> = dag
                .all_tasks()
                .filter(|task| !task.status.is_terminal())
                .filter_map(|task| task.id)
                .collect();
            for id in pending {
                if !dag.get_task(id).expect("Task exists").status.is_terminal() {
                    dag.skip_task(id)?;
                }
            }
        }
        if !dag.is_complete() {
            bail!("DAG stopped with unresolved tasks; execution did not complete");
        }
        let counts = dag.status_counts();
        let mut stats = self.stats.lock().await;
        stats.total_tasks = dag.len();
        stats.completed_tasks = *counts.get(&TaskStatus::Completed).unwrap_or(&0);
        stats.failed_tasks = *counts.get(&TaskStatus::Failed).unwrap_or(&0)
            + *counts.get(&TaskStatus::Cancelled).unwrap_or(&0);
        stats.skipped_tasks = *counts.get(&TaskStatus::Skipped).unwrap_or(&0);
        stats.total_duration = start.elapsed();
        Ok(stats.clone())
    }
}
