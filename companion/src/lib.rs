pub mod notify;
pub mod storage;
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SOCKET: &str = "/run/tracelyn/companion.sock";
pub fn evaluate(
    rule: &tracelyn_core::types::IntelRule,
    previous: tracelyn_core::logic::Gate,
    value: f64,
    ts: i64,
) -> tracelyn_core::logic::Gate {
    use tracelyn_core::logic::{advance_gate, clear_level, hot_cool};
    let (hot, cool) = hot_cool(
        &rule.kind,
        value,
        rule.params.threshold,
        clear_level(
            &rule.kind,
            rule.params.threshold,
            rule.params.clear_threshold,
        ),
    );
    advance_gate(
        previous,
        hot,
        cool,
        ts,
        (rule.params.duration_secs.min(i64::MAX as u64 / 1000) * 1000) as i64,
        (rule.params.clear_duration_secs.min(i64::MAX as u64 / 1000) * 1000) as i64,
    )
}
pub fn systemd_unit() -> String {
    "[Unit]\nDescription=Tracelyn server companion\nAfter=network-online.target\nWants=network-online.target\n\n[Service]\nType=simple\nUser=tracelyn\nGroup=tracelyn\nSupplementaryGroups=adm systemd-journal\nExecStart=/usr/local/bin/tracelyn-companion serve\nRestart=on-failure\nRestartSec=5\nStateDirectory=tracelyn\nRuntimeDirectory=tracelyn\nRuntimeDirectoryMode=0750\nUMask=0077\nNoNewPrivileges=yes\nProtectSystem=strict\nProtectHome=read-only\nReadWritePaths=/var/lib/tracelyn /run/tracelyn\nPrivateTmp=yes\nMemoryMax=64M\nRestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\nCapabilityBoundingSet=\nProtectKernelTunables=yes\nProtectControlGroups=yes\n\n[Install]\nWantedBy=multi-user.target\n".into()
}
#[cfg(test)]
mod tests {
    #[test]
    fn hardened_unit() {
        let unit = super::systemd_unit();
        for required in [
            "User=tracelyn",
            "SupplementaryGroups=adm systemd-journal",
            "NoNewPrivileges=yes",
            "ProtectSystem=strict",
            "ReadWritePaths=/var/lib/tracelyn /run/tracelyn",
            "PrivateTmp=yes",
            "MemoryMax=64M",
        ] {
            assert!(unit.lines().any(|line| line == required));
        }
        assert!(!unit.contains("ListenStream"));
    }
}
