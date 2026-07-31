#![forbid(unsafe_code)]

mod diff;
mod editor;
mod engine;
mod model;
mod parser;
mod tool;
mod translate;

pub use diff::{DiffLine, DiffResult, diff_lines};
pub use engine::{Baseline, MemoryBaseline, evaluate, evaluate_peek};
pub use model::{
    Change, ChangeKind, ChangeSet, CommandError, CommandGroupError, Evaluation, Instruction,
    Operation, Program,
};
pub use parser::{PhysicalLine, clean_path, parse, split_physical_lines};
pub use tool::{TOOL_DESCRIPTION_COMPACT, TOOL_GRAMMAR};
pub use translate::translate_apply_patch;
