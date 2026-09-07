use anyhow::{Result, bail};

use super::RunCli;

impl RunCli {
    pub(crate) fn validate_runtime_options(&self) -> Result<()> {
        if self.attach.is_some() {
            bail!("--attach is not supported by this runtime. No local session was started.");
        }
        let unsupported = [
            (self.schema.is_some(), "--schema"),
            (self.temperature.is_some(), "--temperature"),
            (self.top_p.is_some(), "--top-p"),
            (self.top_k.is_some(), "--top-k"),
            (self.seed.is_some(), "--seed"),
            (self.max_tokens.is_some(), "--max-tokens"),
            (self.frequency_penalty.is_some(), "--frequency-penalty"),
            (self.presence_penalty.is_some(), "--presence-penalty"),
            (!self.stop_sequences.is_empty(), "--stop"),
            (self.logprobs.is_some(), "--logprobs"),
            (self.num_completions.is_some(), "--n"),
            (self.best_of.is_some(), "--best-of"),
            (self.retry != 0, "--retry"),
            (self.no_cache, "--no-cache"),
            (self.port.is_some(), "--port"),
            (!self.add_dir.is_empty(), "--add-dir"),
            (self.title.is_some(), "--title"),
            (self.share, "--share"),
        ];
        if let Some((_, flag)) = unsupported.into_iter().find(|(set, _)| *set) {
            bail!("{flag} is not supported by the Code service contract. No turn was submitted.");
        }
        Ok(())
    }
}
