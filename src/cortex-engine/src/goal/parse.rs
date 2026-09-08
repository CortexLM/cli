//! Slash-command parsing for `/goal`.

use super::types::GoalCommand;

/// Parse `/goal` arguments (already split, command name stripped).
///
/// Reserved single tokens: `pause`, `resume`, `clear`. Anything else
/// (including the empty list) is status or a new objective.
pub fn parse_goal_args(args: &[String]) -> Result<GoalCommand, String> {
    if args.is_empty() {
        return Ok(GoalCommand::Status);
    }

    if args.len() == 1 {
        match args[0].to_ascii_lowercase().as_str() {
            "pause" => return Ok(GoalCommand::Pause),
            "resume" => return Ok(GoalCommand::Resume),
            "clear" => return Ok(GoalCommand::Clear),
            other if other.trim().is_empty() => {
                return Err("Goal objective cannot be empty.".to_string());
            }
            _ => {}
        }
    }

    let objective = args.join(" ");
    let objective = objective.trim();
    if objective.is_empty() {
        return Err("Goal objective cannot be empty.".to_string());
    }
    Ok(GoalCommand::Set {
        objective: objective.to_string(),
    })
}

/// Parse a raw composer line such as `/goal pause the deploy`.
pub fn parse_goal_input(input: &str) -> Result<GoalCommand, String> {
    let input = input.trim();
    let rest = if let Some(rest) = input.strip_prefix("/goal") {
        rest
    } else if let Some(rest) = input.strip_prefix("/GOAL") {
        rest
    } else {
        return Err("Not a /goal command.".to_string());
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(GoalCommand::Status);
    }
    parse_goal_args(&split_args(rest))
}

/// Whitespace split that keeps quoted groups together.
fn split_args(args_str: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = None::<char>;
    for c in args_str.chars() {
        match (c, in_quote) {
            ('"' | '\'', None) => in_quote = Some(c),
            (q, Some(open)) if q == open => in_quote = None,
            (c, None) if c.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (c, _) => current.push(c),
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_status() {
        assert_eq!(parse_goal_args(&[]).unwrap(), GoalCommand::Status);
        assert_eq!(parse_goal_input("/goal").unwrap(), GoalCommand::Status);
        assert_eq!(parse_goal_input("  /goal  ").unwrap(), GoalCommand::Status);
    }

    #[test]
    fn reserved_tokens() {
        assert_eq!(parse_goal_input("/goal pause").unwrap(), GoalCommand::Pause);
        assert_eq!(
            parse_goal_input("/goal resume").unwrap(),
            GoalCommand::Resume
        );
        assert_eq!(parse_goal_input("/goal clear").unwrap(), GoalCommand::Clear);
        assert_eq!(parse_goal_input("/GOAL PAUSE").unwrap(), GoalCommand::Pause);
    }

    #[test]
    fn set_objective() {
        assert_eq!(
            parse_goal_input("/goal create foo.txt and assert it").unwrap(),
            GoalCommand::Set {
                objective: "create foo.txt and assert it".into()
            }
        );
        assert_eq!(
            parse_goal_input("/goal pause the deploy").unwrap(),
            GoalCommand::Set {
                objective: "pause the deploy".into()
            }
        );
    }

    #[test]
    fn quoted_objective() {
        assert_eq!(
            parse_goal_input("/goal \"fix the flaky test\"").unwrap(),
            GoalCommand::Set {
                objective: "fix the flaky test".into()
            }
        );
    }

    #[test]
    fn rejects_non_goal() {
        assert!(parse_goal_input("/plan").is_err());
        assert!(parse_goal_input("goal").is_err());
    }
}
