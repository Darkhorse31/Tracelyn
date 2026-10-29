mod audit;
mod plan;
mod remote;

pub use audit::{
    list_audit, operations_enabled, record, set_operations_enabled, to_csv, AuditQuery, AuditRow,
};
pub use plan::{
    actor_name, excerpt, health_message, health_reached, needs_sudo_stdin, plan_action, probe_is_up,
    shell_quote,
    step_should_run, PlanInput, PlannedAction,
};
pub use remote::{run_shell, OpsRuntime};

pub fn migrate(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    audit::migrate(conn)
}
