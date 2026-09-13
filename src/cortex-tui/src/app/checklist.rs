//! Session working checklist (lock `todos`).

use super::subagent::{SubagentTodoItem, SubagentTodoStatus};

/// Live turn checklist painted above the composer — distinct from subagent tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingChecklist {
    pub elapsed_secs: u32,
    pub tokens: u64,
    pub items: Vec<SubagentTodoItem>,
}

impl WorkingChecklist {
    pub fn new(items: Vec<SubagentTodoItem>, elapsed_secs: u32, tokens: u64) -> Self {
        Self {
            elapsed_secs,
            tokens,
            items,
        }
    }

    pub fn done(&self) -> usize {
        self.items
            .iter()
            .filter(|t| matches!(t.status, SubagentTodoStatus::Completed))
            .count()
    }

    pub fn header(&self) -> String {
        format!(
            "⠇ Working {}/{}  ·  {}s · {}",
            self.done(),
            self.items.len(),
            self.elapsed_secs,
            compact_tokens(self.tokens)
        )
    }
}

fn compact_tokens(n: u64) -> String {
    if n >= 1_000 {
        let k = n as f64 / 1_000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{}k tokens", k as u64)
        } else {
            format!("{k:.1}k tokens")
        }
    } else {
        format!("{n} tokens")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_matches_lock_copy() {
        let board = WorkingChecklist::new(
            vec![
                SubagentTodoItem::new("a", SubagentTodoStatus::Completed),
                SubagentTodoItem::new("b", SubagentTodoStatus::Completed),
                SubagentTodoItem::new("c", SubagentTodoStatus::InProgress),
                SubagentTodoItem::new("d", SubagentTodoStatus::Pending),
                SubagentTodoItem::new("e", SubagentTodoStatus::Pending),
            ],
            38,
            6_100,
        );
        assert_eq!(board.header(), "⠇ Working 2/5  ·  38s · 6.1k tokens");
    }
}
