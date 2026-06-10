use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_LAYER_ALE_AUTH_CONNECT_V4;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_LAYER_ALE_AUTH_CONNECT_V6;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_LAYER_ALE_RESOURCE_ASSIGNMENT_V4;
use windows_sys::Win32::NetworkManagement::WindowsFilteringPlatform::FWPM_LAYER_ALE_RESOURCE_ASSIGNMENT_V6;
use windows_sys::Win32::Networking::WinSock::IPPROTO_ICMP;
use windows_sys::Win32::Networking::WinSock::IPPROTO_ICMPV6;
use windows_sys::core::GUID;

#[derive(Clone, Copy)]
pub(super) enum ConditionSpec {
    User,
    Protocol(u8),
    RemotePort(u16),
    /// Remote address in 127.0.0.0/8.
    RemoteLoopbackV4,
    /// Remote address ::1/128.
    RemoteLoopbackV6,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FilterAction {
    Block,
    Permit,
}

#[derive(Clone, Copy)]
pub(super) struct FilterSpec {
    pub(super) key: GUID,
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) layer_key: GUID,
    pub(super) action: FilterAction,
    /// `FWP_UINT8` weight bucket (0–15); `None` lets BFE auto-assign. The
    /// loopback permits must out-weigh every block so local IPC keeps working.
    pub(super) weight: Option<u8>,
    pub(super) conditions: &'static [ConditionSpec],
}

pub(super) const FILTER_SPECS: &[FilterSpec] = &[
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004011b0b000000000011),
        name: "Zagens_wfp_icmp_connect_v4",
        description: "Block sandbox-account ICMP connect v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[
            ConditionSpec::User,
            ConditionSpec::Protocol(IPPROTO_ICMP as u8),
        ],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004012b0b000000000012),
        name: "Zagens_wfp_icmp_connect_v6",
        description: "Block sandbox-account ICMP connect v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[
            ConditionSpec::User,
            ConditionSpec::Protocol(IPPROTO_ICMPV6 as u8),
        ],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004013b0b000000000013),
        name: "Zagens_wfp_icmp_assign_v4",
        description: "Block sandbox-account ICMP resource assignment v4",
        layer_key: FWPM_LAYER_ALE_RESOURCE_ASSIGNMENT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[
            ConditionSpec::User,
            ConditionSpec::Protocol(IPPROTO_ICMP as u8),
        ],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004014b0b000000000014),
        name: "Zagens_wfp_icmp_assign_v6",
        description: "Block sandbox-account ICMP resource assignment v6",
        layer_key: FWPM_LAYER_ALE_RESOURCE_ASSIGNMENT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[
            ConditionSpec::User,
            ConditionSpec::Protocol(IPPROTO_ICMPV6 as u8),
        ],
    },
    // NAME_RESOLUTION_CACHE filters are intentionally omitted because ordinary
    // static filter shapes returned FWP_E_OUT_OF_BOUNDS during validation.
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004021b0b000000000021),
        name: "Zagens_wfp_dns_53_v4",
        description: "Block sandbox-account DNS TCP or UDP port 53 v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(53)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004022b0b000000000022),
        name: "Zagens_wfp_dns_53_v6",
        description: "Block sandbox-account DNS TCP or UDP port 53 v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(53)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004023b0b000000000023),
        name: "Zagens_wfp_dns_853_v4",
        description: "Block sandbox-account DNS-over-TLS port 853 v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(853)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004024b0b000000000024),
        name: "Zagens_wfp_dns_853_v6",
        description: "Block sandbox-account DNS-over-TLS port 853 v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(853)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004031b0b000000000031),
        name: "Zagens_wfp_smb_445_v4",
        description: "Block sandbox-account SMB port 445 v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(445)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004032b0b000000000032),
        name: "Zagens_wfp_smb_445_v6",
        description: "Block sandbox-account SMB port 445 v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(445)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004033b0b000000000033),
        name: "Zagens_wfp_smb_139_v4",
        description: "Block sandbox-account SMB port 139 v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(139)],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004034b0b000000000034),
        name: "Zagens_wfp_smb_139_v6",
        description: "Block sandbox-account SMB port 139 v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: None,
        conditions: &[ConditionSpec::User, ConditionSpec::RemotePort(139)],
    },
    // Offline semantics require a catch-all outbound block: protocol/port
    // blocks above cannot cover e.g. plain TCP 80, and the DNS 53 block never
    // matches anyway because resolution runs in the Dnscache service, not in
    // the sandbox account. Loopback stays permitted (top weight bucket) so
    // local runtime/sidecar IPC keeps working (design §10.2 NO_PROXY intent).
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004041b0b000000000041),
        name: "Zagens_wfp_loopback_permit_v4",
        description: "Permit sandbox-account loopback connect v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Permit,
        weight: Some(15),
        conditions: &[ConditionSpec::User, ConditionSpec::RemoteLoopbackV4],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004042b0b000000000042),
        name: "Zagens_wfp_loopback_permit_v6",
        description: "Permit sandbox-account loopback connect v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Permit,
        weight: Some(15),
        conditions: &[ConditionSpec::User, ConditionSpec::RemoteLoopbackV6],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004043b0b000000000043),
        name: "Zagens_wfp_outbound_all_v4",
        description: "Block sandbox-account all other outbound connect v4",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V4,
        action: FilterAction::Block,
        weight: Some(1),
        conditions: &[ConditionSpec::User],
    },
    FilterSpec {
        key: GUID::from_u128(0x7a676e7300004044b0b000000000044),
        name: "Zagens_wfp_outbound_all_v6",
        description: "Block sandbox-account all other outbound connect v6",
        layer_key: FWPM_LAYER_ALE_AUTH_CONNECT_V6,
        action: FilterAction::Block,
        weight: Some(1),
        conditions: &[ConditionSpec::User],
    },
];
