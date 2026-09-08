//! Claude is a proposer. Validation V0–V6 lives here. No broker I/O.

mod claude;
mod error;
mod extract;
mod suggest;
mod validate;

pub use claude::{ClaudeClient, Llm, LlmReq, MockLlm};
pub use error::PolicyError;
pub use extract::extract_tool_input;
pub use suggest::{
    NameSuggest, SuggestSide, hold_no_signal, hold_rejected, hold_stale, suggest_with_one_retry,
    validate_suggest,
};
pub use validate::{
    LastGood, LiveRefs, TokenBudget, quant_with_one_retry, validate_policy, validate_ticket,
};
