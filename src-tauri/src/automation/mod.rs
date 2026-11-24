//! Work Poros does on its own: commands the user defines for servers, what happens when the
//! transfer queue finishes, and scheduled tasks. The tasks are kept and run by code the app and
//! the command line share (`task`, `board`, `runner`); `scheduler` is the app's part.

pub mod board;
pub mod lock;
pub mod remote_command;
pub mod runner;
pub mod schedule;
#[cfg(feature = "gui")]
pub mod scheduler;
pub mod system;
pub mod task;
mod transfer_task;
